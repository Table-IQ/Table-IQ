-- Task 2 (prod-ready): QA seed for tableiq-pg. Adds schemas, cross-schema FKs,
-- composite keys/FKs, and the specific tricky shapes probed in
-- docs/finalize/assets/prod-ready/baseline.md (json-first ORDER BY, an
-- un-ANALYZEd table, etc). Idempotent: safe to re-run. Does NOT touch
-- public.wide (docker/postgres/seed.sql, Task 1).
--
-- Apply: docker exec -i tableiq-pg psql -U postgres -d tableiq -v ON_ERROR_STOP=1 < docker/postgres/qa.sql

-- ── drop (children first) ──────────────────────────────────────────────────
DROP MATERIALIZED VIEW IF EXISTS public.order_totals CASCADE;
DROP VIEW IF EXISTS public.customer_orders CASCADE;
DROP TABLE IF EXISTS billing.invoices CASCADE;
DROP TABLE IF EXISTS public.shipments CASCADE;
DROP TABLE IF EXISTS public.order_items CASCADE;
DROP TABLE IF EXISTS public.orders CASCADE;
DROP TABLE IF EXISTS public.customers CASCADE;
DROP TABLE IF EXISTS public.json_first CASCADE;
DROP TABLE IF EXISTS public.geo CASCADE;
DROP TABLE IF EXISTS public.fresh CASCADE;
DROP TABLE IF EXISTS public."Order ""Log""" CASCADE;
DROP TABLE IF EXISTS public.wide873 CASCADE;
DROP TYPE IF EXISTS public.customer_status CASCADE;
DROP FUNCTION IF EXISTS public.set_order_note() CASCADE;

DROP SCHEMA IF EXISTS auth CASCADE;
DROP SCHEMA IF EXISTS billing CASCADE;
DROP SCHEMA IF EXISTS "Mixed Case" CASCADE;
DROP SCHEMA IF EXISTS empty_schema CASCADE;

-- ── schemas ─────────────────────────────────────────────────────────────────
CREATE SCHEMA auth;          -- name sorts before "public"
CREATE SCHEMA billing;
CREATE SCHEMA "Mixed Case";
CREATE SCHEMA empty_schema;  -- deliberately left empty (introspect edge case)

-- ── auth.users: self-referencing FK, jsonb, ~50 rows with some NULL manager ─
CREATE TABLE auth.users (
  id bigint PRIMARY KEY,
  email text NOT NULL,
  manager_id bigint REFERENCES auth.users(id),
  profile jsonb,
  created_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO auth.users (id, email, manager_id, profile, created_at)
SELECT
  g,
  'user' || g || '@example.com',
  CASE WHEN g <= 5 OR g % 9 = 0 THEN NULL ELSE g - 1 END,
  jsonb_build_object('role', CASE WHEN g % 5 = 0 THEN 'admin' ELSE 'member' END, 'active', (g % 2 = 0)),
  now() - (g || ' days')::interval
FROM generate_series(1, 50) AS g;

-- ── public.customer_status + public.customers: cross-schema FK, arrays, uuid,
-- numeric, inet, interval, enum, bytea with non-UTF-8 bytes; ~200 rows ────────
CREATE TYPE public.customer_status AS ENUM ('active', 'inactive', 'pending', 'banned');

CREATE TABLE public.customers (
  id bigserial PRIMARY KEY,
  user_id bigint REFERENCES auth.users(id),
  name text NOT NULL,
  nickname text,
  tags text[],
  uuid uuid NOT NULL DEFAULT gen_random_uuid(),
  balance numeric(12, 2) NOT NULL DEFAULT 0,
  ip inet,
  session interval,
  status public.customer_status NOT NULL DEFAULT 'active',
  avatar bytea
);

INSERT INTO public.customers (user_id, name, nickname, tags, balance, ip, session, status, avatar)
SELECT
  ((g - 1) % 50) + 1,
  'Customer ' || g,
  CASE WHEN g % 3 = 0 THEN NULL WHEN g % 3 = 1 THEN '' ELSE 'nick' || g END,
  ARRAY['tag' || (g % 5), 'cust'],
  (random() * 10000)::numeric(12, 2),
  ('192.168.' || (g % 256) || '.' || ((g * 7) % 256))::inet,
  (g || ' minutes')::interval,
  (ARRAY['active', 'inactive', 'pending', 'banned']::public.customer_status[])[1 + (g % 4)],
  CASE WHEN g % 5 = 0 THEN '\xdeadbeef'::bytea ELSE decode(md5(g::text), 'hex') END
FROM generate_series(1, 200) AS g;

-- "a unique index on customers(email_like_col) or similar" — the customers
-- shape above has no email column, so we index the closest per-row-unique
-- column instead (uuid) rather than inventing a column the task didn't name.
CREATE UNIQUE INDEX customers_uuid_key ON public.customers (uuid);
COMMENT ON COLUMN public.customers.balance IS 'Running account balance in local currency.';

-- ── public.orders (+ trigger that sets note on insert) ──────────────────────
CREATE TABLE public.orders (
  id bigint PRIMARY KEY,
  customer_id bigint REFERENCES public.customers(id),
  total numeric(12, 2),
  placed_at timestamptz,
  note text
);

CREATE FUNCTION public.set_order_note() RETURNS trigger AS $$
BEGIN
  NEW.note := coalesce(NEW.note, 'order placed for customer ' || NEW.customer_id);
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_orders_set_note
  BEFORE INSERT ON public.orders
  FOR EACH ROW EXECUTE FUNCTION public.set_order_note();

INSERT INTO public.orders (id, customer_id, total, placed_at)
SELECT
  g,
  ((g - 1) % 200) + 1,
  (random() * 500)::numeric(12, 2),
  now() - (g || ' hours')::interval
FROM generate_series(1, 300) AS g;

-- ── public.order_items: composite PK ─────────────────────────────────────────
CREATE TABLE public.order_items (
  order_id bigint REFERENCES public.orders(id),
  line_no int,
  sku text,
  qty int,
  price numeric(10, 2),
  PRIMARY KEY (order_id, line_no)
);

INSERT INTO public.order_items (order_id, line_no, sku, qty, price)
SELECT o.g, l.line_no, 'SKU-' || o.g || '-' || l.line_no, 1 + (o.g % 4), (random() * 100)::numeric(10, 2)
FROM generate_series(1, 300) AS o(g)
CROSS JOIN generate_series(1, 3) AS l(line_no)
WHERE l.line_no <= 1 + (o.g % 3);

-- ── public.shipments: composite FK into order_items ──────────────────────────
CREATE TABLE public.shipments (
  id serial PRIMARY KEY,
  order_id bigint,
  line_no int,
  FOREIGN KEY (order_id, line_no) REFERENCES public.order_items(order_id, line_no)
);

INSERT INTO public.shipments (order_id, line_no)
SELECT order_id, line_no FROM public.order_items WHERE line_no = 1;

-- ── billing.invoices: cross-schema FK into public.orders ─────────────────────
CREATE TABLE billing.invoices (
  id serial PRIMARY KEY,
  order_id bigint REFERENCES public.orders(id),
  amount numeric,
  issued date
);

INSERT INTO billing.invoices (order_id, amount, issued)
SELECT id, total, placed_at::date FROM public.orders;

-- ── public.json_first: json-typed first column, no PK — json has no default
-- btree opclass, so `ORDER BY 1` (get_table_window's windowing query) fails ──
CREATE TABLE public.json_first (
  payload json,
  label text
);

INSERT INTO public.json_first (payload, label)
SELECT ('{"n":' || g || '}')::json, 'row ' || g
FROM generate_series(1, 20) AS g;

-- ── public.geo: point + xml ───────────────────────────────────────────────────
CREATE TABLE public.geo (
  id int PRIMARY KEY,
  loc point,
  doc xml
);

INSERT INTO public.geo (id, loc, doc)
SELECT g, point(g, g * 2), ('<note>' || g || '</note>')::xml
FROM generate_series(1, 10) AS g;

-- ── public.fresh: autovacuum disabled + never ANALYZEd — pg_class.reltuples
-- stays 0, so row_estimate_filtered() under-reports the true 500 rows ────────
CREATE TABLE public.fresh (
  id int PRIMARY KEY,
  v text
) WITH (autovacuum_enabled = false);

INSERT INTO public.fresh (id, v)
SELECT g, 'v' || g FROM generate_series(1, 500) AS g;
-- Deliberately no ANALYZE here.

-- ── public."Order ""Log""": identifier with an embedded double quote ────────
CREATE TABLE public."Order ""Log""" (
  id int PRIMARY KEY,
  msg text
);

INSERT INTO public."Order ""Log""" (id, msg) VALUES
  (1, 'created'),
  (2, 'shipped'),
  (3, 'delivered');

-- ── public.wide873: 872 extra columns via a DO loop, 50 rows ─────────────────
CREATE TABLE public.wide873 (
  id int PRIMARY KEY
);

DO $$
BEGIN
  FOR i IN 1..872 LOOP
    EXECUTE format('ALTER TABLE public.wide873 ADD COLUMN c%s text', i);
  END LOOP;
END $$;

INSERT INTO public.wide873 (id, c1, c2, c3)
SELECT g, 'row-' || g, 'c2-' || g, 'c3-' || g
FROM generate_series(1, 50) AS g;

-- ── views ──────────────────────────────────────────────────────────────────
CREATE VIEW public.customer_orders AS
SELECT c.id AS customer_id, c.name, o.id AS order_id, o.total, o.placed_at
FROM public.customers c
JOIN public.orders o ON o.customer_id = c.id;

CREATE MATERIALIZED VIEW public.order_totals AS
SELECT customer_id, count(*) AS order_count, sum(total) AS total_spent
FROM public.customer_orders
GROUP BY customer_id;

-- ── "Mixed Case"."Weird Table": mixed-case schema + table identifiers ───────
CREATE TABLE "Mixed Case"."Weird Table" (
  id int PRIMARY KEY,
  note text
);

INSERT INTO "Mixed Case"."Weird Table" (id, note) VALUES
  (1, 'first'),
  (2, 'second');
