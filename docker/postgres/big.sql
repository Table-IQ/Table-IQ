-- Scale fixture (NOT auto-loaded): 20M real rows in a 4-way partitioned table whose
-- planner stats are faked to ~1 trillion rows, plus a 2M-row table with no PK.
-- Load: docker exec -i tableiq-pg psql -U postgres -d tableiq < docker/postgres/big.sql
DROP SCHEMA IF EXISTS big CASCADE;
CREATE SCHEMA big;
CREATE TABLE big.events (
  id bigint PRIMARY KEY,
  user_id int NOT NULL,
  kind text NOT NULL,
  payload jsonb,
  created_at timestamptz NOT NULL
) PARTITION BY RANGE (id);
CREATE TABLE big.events_p0 PARTITION OF big.events FOR VALUES FROM (0) TO (5000000);
CREATE TABLE big.events_p1 PARTITION OF big.events FOR VALUES FROM (5000000) TO (10000000);
CREATE TABLE big.events_p2 PARTITION OF big.events FOR VALUES FROM (10000000) TO (15000000);
CREATE TABLE big.events_p3 PARTITION OF big.events FOR VALUES FROM (15000000) TO (20000000);
INSERT INTO big.events
SELECT g, g % 100000, (ARRAY['click','view','buy','signup'])[1 + g % 4],
       jsonb_build_object('n', g, 'tag', 'row-' || g),
       now() - (g || ' seconds')::interval
FROM generate_series(0, 19999999) g;
ANALYZE big.events;
-- Pretend each partition holds 250 billion rows (≈ 1 trillion total).
UPDATE pg_class SET reltuples = 2.5e11
 WHERE oid IN ('big.events_p0'::regclass, 'big.events_p1'::regclass,
               'big.events_p2'::regclass, 'big.events_p3'::regclass);
CREATE TABLE big.nopk AS
SELECT g AS n, md5(g::text) AS h FROM generate_series(1, 2000000) g;
