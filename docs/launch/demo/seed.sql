-- Synthetic storefront for local TableIQ demonstrations. No customer data.
-- Works in a new SQLite file; all amounts are integer cents.
PRAGMA foreign_keys = ON;
CREATE TABLE customers (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL
);
CREATE TABLE orders (
  id INTEGER PRIMARY KEY,
  customer_id INTEGER NOT NULL REFERENCES customers(id),
  ordered_at TEXT NOT NULL,
  total_cents INTEGER NOT NULL
);
CREATE TABLE payments (
  id INTEGER PRIMARY KEY,
  order_id INTEGER NOT NULL REFERENCES orders(id),
  status TEXT NOT NULL
);
CREATE TABLE shipments (
  id INTEGER PRIMARY KEY,
  order_id INTEGER NOT NULL REFERENCES orders(id),
  status TEXT NOT NULL
);
INSERT INTO customers VALUES
  (1, 'Acme Demo'), (2, 'Northstar Demo'), (3, 'Atlas Demo');
INSERT INTO orders VALUES
  (101, 1, '2026-11-01', 2500),
  (102, 2, '2026-11-02', 4200),
  (103, 3, '2026-11-03', 1800),
  (104, 1, '2026-11-04', 3100);
INSERT INTO payments VALUES
  (1, 101, 'paid'), (2, 102, 'paid'),
  (3, 103, 'pending'), (4, 104, 'paid');
INSERT INTO shipments VALUES
  (1, 101, 'shipped'), (2, 103, 'pending');
