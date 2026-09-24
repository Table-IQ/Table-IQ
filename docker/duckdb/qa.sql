-- QA seed for DuckDB. Idempotent: drops+recreates its own objects only.
-- Build: duckdb /tmp/tiq-qa.duckdb < docker/duckdb/qa.sql
-- (or open /tmp/tiq-qa.duckdb in TableIQ and run this file in a query tab)

DROP VIEW IF EXISTS child_view;
DROP TABLE IF EXISTS child;
DROP TABLE IF EXISTS parent;
DROP TABLE IF EXISTS sales.lines;
DROP SCHEMA IF EXISTS sales;

CREATE TABLE parent (
  id INTEGER PRIMARY KEY,
  name VARCHAR NOT NULL
);

INSERT INTO parent (id, name) VALUES
  (1, 'alpha'),
  (2, 'beta'),
  (3, 'gamma');

CREATE TABLE child (
  id INTEGER PRIMARY KEY,
  parent_id INTEGER REFERENCES parent (id),
  blob_data BLOB,
  amount DECIMAL(12, 2),
  tags VARCHAR[],
  created_at TIMESTAMP DEFAULT current_timestamp
);

INSERT INTO child (id, parent_id, blob_data, amount, tags) VALUES
  (1, 1, '\xDE\xAD\xBE\xEF'::BLOB, 12.50, ['a', 'b']),
  (2, 2, '\x00\xFF\x00\xFF'::BLOB, 8.00, []),
  (3, NULL, NULL, NULL, NULL);

CREATE INDEX child_parent_idx ON child (parent_id);

CREATE VIEW child_view AS
SELECT child.id, child.parent_id, parent.name AS parent_name
FROM child
JOIN parent ON parent.id = child.parent_id;

-- A second schema and a composite primary key.
CREATE SCHEMA sales;
CREATE TABLE sales.lines (
  order_id INTEGER,
  line_no INTEGER,
  sku VARCHAR,
  PRIMARY KEY (order_id, line_no)
);
INSERT INTO sales.lines
SELECT i // 10, i % 10, 'sku-' || i FROM range(0, 1000) t(i);
