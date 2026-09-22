-- Task 2 (prod-ready): QA seed for tableiq-mysql (mysql:8.4, port 53306).
-- Mirrors the Postgres qa.sql shapes where MySQL supports them. Idempotent.
-- Auto-applied on first container init (docker-entrypoint-initdb.d); to
-- reapply against a running container:
--   docker exec -i tableiq-mysql mysql -uroot -ptableiq tableiq < docker/mysql/qa.sql

-- ── drop (children first) ────────────────────────────────────────────────
DROP VIEW IF EXISTS customer_orders;
DROP TABLE IF EXISTS shipments;
DROP TABLE IF EXISTS order_items;
DROP TABLE IF EXISTS orders;
DROP TABLE IF EXISTS customers;
DROP TABLE IF EXISTS users;
DROP TABLE IF EXISTS json_first;

-- ── users: self-referencing FK, BIT(1), DATETIME; ~50 rows, some NULL manager
CREATE TABLE users (
  id BIGINT UNSIGNED PRIMARY KEY,
  email VARCHAR(255) NOT NULL,
  manager_id BIGINT UNSIGNED,
  active BIT(1) NOT NULL DEFAULT b'1',
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (manager_id) REFERENCES users(id)
);

INSERT INTO users (id, email, active)
WITH RECURSIVE seq AS (
  SELECT 1 AS n
  UNION ALL
  SELECT n + 1 FROM seq WHERE n < 50
)
SELECT n, CONCAT('user', n, '@example.com'), IF(n % 2 = 0, b'1', b'0')
FROM seq;

-- Two-step self-FK population (safer than relying on multi-row INSERT order).
UPDATE users SET manager_id = id - 1
WHERE id > 5 AND id % 9 <> 0;

-- ── customers: FK, ENUM, DECIMAL, JSON, BLOB with non-UTF-8 bytes, index,
-- a column comment and a default; ~200 rows ─────────────────────────────────
CREATE TABLE customers (
  id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
  user_id BIGINT UNSIGNED,
  name VARCHAR(255) NOT NULL,
  status ENUM('active', 'inactive', 'pending', 'banned') NOT NULL DEFAULT 'active',
  balance DECIMAL(12, 2) NOT NULL DEFAULT 0 COMMENT 'Running account balance',
  meta JSON,
  avatar BLOB,
  FOREIGN KEY (user_id) REFERENCES users(id)
);

INSERT INTO customers (user_id, name, status, balance, meta, avatar)
WITH RECURSIVE seq AS (
  SELECT 1 AS n
  UNION ALL
  SELECT n + 1 FROM seq WHERE n < 200
)
SELECT
  ((n - 1) % 50) + 1,
  CONCAT('Customer ', n),
  ELT(1 + (n % 4), 'active', 'inactive', 'pending', 'banned'),
  ROUND(RAND() * 10000, 2),
  JSON_OBJECT('n', n),
  IF(n % 5 = 0, 0xDEADBEEF, UNHEX(MD5(n)))
FROM seq;

CREATE INDEX customers_name_idx ON customers (name);

-- ── orders (+ trigger that fills placed_at on insert if omitted) ────────────
CREATE TABLE orders (
  id BIGINT UNSIGNED PRIMARY KEY,
  customer_id BIGINT UNSIGNED,
  total DECIMAL(12, 2),
  placed_at DATETIME,
  FOREIGN KEY (customer_id) REFERENCES customers(id)
);

DELIMITER $$
CREATE TRIGGER trg_orders_before_insert
BEFORE INSERT ON orders
FOR EACH ROW
BEGIN
  IF NEW.placed_at IS NULL THEN
    SET NEW.placed_at = NOW();
  END IF;
END$$
DELIMITER ;

INSERT INTO orders (id, customer_id, total, placed_at)
WITH RECURSIVE seq AS (
  SELECT 1 AS n
  UNION ALL
  SELECT n + 1 FROM seq WHERE n < 300
)
SELECT n, ((n - 1) % 200) + 1, ROUND(RAND() * 500, 2), DATE_SUB(NOW(), INTERVAL n HOUR)
FROM seq;

-- ── order_items: composite PK ────────────────────────────────────────────────
CREATE TABLE order_items (
  order_id BIGINT UNSIGNED,
  line_no INT,
  sku VARCHAR(64),
  qty INT,
  price DECIMAL(10, 2),
  PRIMARY KEY (order_id, line_no),
  FOREIGN KEY (order_id) REFERENCES orders(id)
);

INSERT INTO order_items (order_id, line_no, sku, qty, price)
WITH RECURSIVE seq AS (
  SELECT 1 AS n
  UNION ALL
  SELECT n + 1 FROM seq WHERE n < 300
)
SELECT seq.n, l.line_no, CONCAT('SKU-', seq.n, '-', l.line_no), 1 + (seq.n % 4), ROUND(RAND() * 100, 2)
FROM seq
JOIN (SELECT 1 AS line_no UNION ALL SELECT 2 UNION ALL SELECT 3) AS l
  ON l.line_no <= 1 + (seq.n % 3);

-- ── shipments: composite FK into order_items ─────────────────────────────────
CREATE TABLE shipments (
  id INT AUTO_INCREMENT PRIMARY KEY,
  order_id BIGINT UNSIGNED,
  line_no INT,
  FOREIGN KEY (order_id, line_no) REFERENCES order_items(order_id, line_no)
);

INSERT INTO shipments (order_id, line_no)
SELECT order_id, line_no FROM order_items WHERE line_no = 1;

-- ── json_first: JSON-typed first column, no PK ───────────────────────────────
CREATE TABLE json_first (
  payload JSON,
  label VARCHAR(255)
);

INSERT INTO json_first (payload, label)
WITH RECURSIVE seq AS (
  SELECT 1 AS n
  UNION ALL
  SELECT n + 1 FROM seq WHERE n < 20
)
SELECT JSON_OBJECT('n', n), CONCAT('row ', n)
FROM seq;

-- ── view ──────────────────────────────────────────────────────────────────
CREATE VIEW customer_orders AS
SELECT c.id AS customer_id, c.name, o.id AS order_id, o.total, o.placed_at
FROM customers c
JOIN orders o ON o.customer_id = c.id;
