-- Seed for tests/libsql_live.rs. Single-quoted strings only (see seed.sh). Applied once; the
-- marker table is the last thing created.
BEGIN;
CREATE TABLE types (id INTEGER PRIMARY KEY, i_max INTEGER, i_min INTEGER, r REAL, r_big REAL, t TEXT NOT NULL, b BLOB, n TEXT, num NUMERIC, mixed);
INSERT INTO types VALUES (1, 9223372036854775807, -9223372036854775808, 0.1, 1e21, 'héllo, wörld 🚀', x'00ff10deadbeef', NULL, 12.50, 'text');
INSERT INTO types VALUES (2, 0, 0, 3.0, -0.000000015, '', x'', NULL, 7, 42);
INSERT INTO types VALUES (3, 1, -1, 2.5, 123456789.125, 'a'||char(9)||'b'||char(10)||'c', x'6869', 'present', 0, 1.5);
CREATE TABLE events (id INTEGER PRIMARY KEY, kind TEXT NOT NULL, amount REAL, payload TEXT, ts TEXT);
WITH RECURSIVE c(x) AS (SELECT 0 UNION ALL SELECT x + 1 FROM c WHERE x < 19999) INSERT INTO events SELECT x, CASE x % 4 WHEN 0 THEN 'view' WHEN 1 THEN 'click' WHEN 2 THEN 'buy' ELSE 'share' END, (x % 1000) / 4.0, 'event-' || x, datetime(1704067200 + x, 'unixepoch') FROM c;
CREATE INDEX idx_events_kind ON events (kind);
CREATE TABLE orders (region TEXT NOT NULL, id INTEGER NOT NULL, total REAL NOT NULL, note TEXT, PRIMARY KEY (region, id)) WITHOUT ROWID;
INSERT INTO orders SELECT r.region, i.x, ((r.k * 10 + i.x) * 7 % 100) + 0.5, CASE WHEN i.x % 5 = 4 THEN NULL ELSE 'note ' || i.x END FROM (SELECT 'east' AS region, 0 AS k UNION ALL SELECT 'north', 1 UNION ALL SELECT 'south', 2 UNION ALL SELECT 'west', 3) r, (WITH RECURSIVE c(x) AS (SELECT 0 UNION ALL SELECT x + 1 FROM c WHERE x < 9) SELECT x FROM c) i;
CREATE VIEW big_orders AS SELECT region, id, total FROM orders WHERE total > 50;
CREATE TABLE customers (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
CREATE UNIQUE INDEX idx_customers_name ON customers (name);
INSERT INTO customers VALUES (1, 'Ada'), (2, 'Grace');
CREATE TABLE invoices (id INTEGER PRIMARY KEY, customer_id INTEGER NOT NULL REFERENCES customers (id), amount REAL);
INSERT INTO invoices VALUES (1, 1, 10.5), (2, 2, 20.25);
CREATE TRIGGER invoices_ai AFTER INSERT ON invoices BEGIN UPDATE customers SET name = name WHERE id = NEW.customer_id; END;
CREATE TABLE shipments (id INTEGER PRIMARY KEY, region TEXT NOT NULL, order_id INTEGER NOT NULL, FOREIGN KEY (region, order_id) REFERENCES orders (region, id));
INSERT INTO shipments VALUES (1, 'east', 0);
CREATE TABLE _tiq_meta (k TEXT PRIMARY KEY, v TEXT);
INSERT INTO _tiq_meta VALUES ('seeded', '1');
COMMIT;
