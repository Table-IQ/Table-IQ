-- Task 4 (prod-ready): QA seed for the SQL Server container. Mirrors the shapes in
-- docker/postgres/qa.sql and docker/mysql/qa.sql where SQL Server supports them:
-- a self-FK, a cross-schema FK, a composite PK and composite FK, a view, a trigger,
-- an index, a column default and a comment, plus date/time/xml/uniqueidentifier/decimal
-- columns and NULLs. Idempotent: safe to re-run.
--
-- Apply (batches are separated by GO, so it goes through sqlcmd, not the app):
--   docker exec -i tableiq-mssql /opt/mssql-tools18/bin/sqlcmd \
--     -S localhost -U sa -P TableIQ_pw1 -C -b < docker/sqlserver/qa.sql
--
-- The container itself:
--   docker run -d --name tableiq-mssql --platform linux/amd64 -e ACCEPT_EULA=Y \
--     -e MSSQL_SA_PASSWORD=TableIQ_pw1 -p 51433:1433 mcr.microsoft.com/mssql/server:2022-latest
-- (mcr.microsoft.com/azure-sql-edge is the arm64-native image, but it SIGABRTs during the
-- master-database upgrade on macOS 27 / Apple Silicon; the amd64 image runs under Rosetta.)

IF DB_ID('tableiq') IS NULL CREATE DATABASE tableiq;
GO
USE tableiq;
GO

-- ── drop (children first) ───────────────────────────────────────────────────
DROP VIEW IF EXISTS dbo.customer_orders;
DROP TABLE IF EXISTS billing.invoices;
DROP TABLE IF EXISTS dbo.shipments;
DROP TABLE IF EXISTS dbo.order_items;
DROP TABLE IF EXISTS dbo.orders;
DROP TABLE IF EXISTS dbo.customers;
DROP TABLE IF EXISTS auth.users;
DROP TABLE IF EXISTS dbo.geo;
DROP TABLE IF EXISTS dbo.[Order ]]Log];
GO

-- ── schemas (CREATE SCHEMA must be the only statement in its batch) ─────────
IF SCHEMA_ID('auth') IS NULL EXEC('CREATE SCHEMA auth');      -- sorts before dbo
GO
IF SCHEMA_ID('billing') IS NULL EXEC('CREATE SCHEMA billing');
GO
IF SCHEMA_ID('staging') IS NULL EXEC('CREATE SCHEMA staging'); -- left empty on purpose
GO

-- ── auth.users: self-referencing FK, datetimeoffset default, ~50 rows with
-- some NULL manager ─────────────────────────────────────────────────────────
CREATE TABLE auth.users (
  id         BIGINT PRIMARY KEY,
  email      NVARCHAR(255) NOT NULL,
  manager_id BIGINT NULL CONSTRAINT fk_users_manager REFERENCES auth.users (id),
  profile    NVARCHAR(MAX) NULL,
  created_at DATETIMEOFFSET NOT NULL CONSTRAINT df_users_created DEFAULT SYSDATETIMEOFFSET()
);
GO

WITH seq AS (
  SELECT TOP (50) ROW_NUMBER() OVER (ORDER BY (SELECT NULL)) AS n FROM sys.all_objects
)
INSERT INTO auth.users (id, email, manager_id, profile, created_at)
SELECT n,
       CONCAT('user', n, '@example.com'),
       CASE WHEN n <= 5 OR n % 9 = 0 THEN NULL ELSE n - 1 END,
       CONCAT('{"role":"', IIF(n % 5 = 0, 'admin', 'member'), '","active":', IIF(n % 2 = 0, 'true', 'false'), '}'),
       DATEADD(DAY, -n, SYSDATETIMEOFFSET())
FROM seq;
GO

-- ── dbo.customers: cross-schema FK into auth.users, uniqueidentifier, decimal,
-- xml, varbinary, and NULL/'' nicknames; ~200 rows ──────────────────────────
CREATE TABLE dbo.customers (
  id       BIGINT IDENTITY(1,1) PRIMARY KEY,
  user_id  BIGINT NULL CONSTRAINT fk_customers_user REFERENCES auth.users (id),
  name     NVARCHAR(255) NOT NULL,
  nickname NVARCHAR(255) NULL,
  uid      UNIQUEIDENTIFIER NOT NULL CONSTRAINT df_customers_uid DEFAULT NEWID(),
  balance  DECIMAL(12, 2) NOT NULL CONSTRAINT df_customers_balance DEFAULT 0,
  status   NVARCHAR(20) NOT NULL CONSTRAINT df_customers_status DEFAULT 'active',
  doc      XML NULL,
  avatar   VARBINARY(MAX) NULL
);
GO

WITH seq AS (
  SELECT TOP (200) ROW_NUMBER() OVER (ORDER BY (SELECT NULL)) AS n FROM sys.all_objects
)
INSERT INTO dbo.customers (user_id, name, nickname, balance, status, doc, avatar)
SELECT ((n - 1) % 50) + 1,
       CONCAT('Customer ', n),
       CASE n % 3 WHEN 0 THEN NULL WHEN 1 THEN N'' ELSE CONCAT('nick', n) END,
       -- A negative balance every 7th row: tiberius' own decimal Display gets the sign wrong.
       CAST(IIF(n % 7 = 0, -1, 1) * (n * 37 % 10000) / 100.0 AS DECIMAL(12, 2)),
       CHOOSE(1 + (n % 4), 'active', 'inactive', 'pending', 'banned'),
       CAST(CONCAT('<customer n="', n, '"/>') AS XML),
       IIF(n % 5 = 0, 0xDEADBEEF, CAST(CONCAT('avatar-', n) AS VARBINARY(MAX)))
FROM seq;
GO

CREATE UNIQUE INDEX customers_uid_key ON dbo.customers (uid);
GO

EXEC sys.sp_addextendedproperty
  @name = N'MS_Description', @value = N'Running account balance in local currency.',
  @level0type = N'SCHEMA', @level0name = N'dbo',
  @level1type = N'TABLE',  @level1name = N'customers',
  @level2type = N'COLUMN', @level2name = N'balance';
GO

-- ── dbo.orders (+ an AFTER INSERT trigger that fills note; SQL Server has no
-- BEFORE triggers) ──────────────────────────────────────────────────────────
CREATE TABLE dbo.orders (
  id          BIGINT PRIMARY KEY,
  customer_id BIGINT NULL CONSTRAINT fk_orders_customer REFERENCES dbo.customers (id),
  total       DECIMAL(12, 2) NULL,
  placed_at   DATETIME2(3) NULL,
  note        NVARCHAR(255) NULL
);
GO

CREATE TRIGGER dbo.trg_orders_set_note ON dbo.orders AFTER INSERT AS
BEGIN
  SET NOCOUNT ON;
  UPDATE o
     SET note = CONCAT('order placed for customer ', i.customer_id)
    FROM dbo.orders o
    JOIN inserted i ON i.id = o.id
   WHERE o.note IS NULL;
END;
GO

WITH seq AS (
  SELECT TOP (300) ROW_NUMBER() OVER (ORDER BY (SELECT NULL)) AS n FROM sys.all_objects
)
INSERT INTO dbo.orders (id, customer_id, total, placed_at)
SELECT n,
       ((n - 1) % 200) + 1,
       CAST((n * 53 % 50000) / 100.0 AS DECIMAL(12, 2)),
       DATEADD(HOUR, -n, CAST('2026-09-23T13:45:07.123' AS DATETIME2(3)))
FROM seq;
GO

-- ── dbo.order_items: composite PK ──────────────────────────────────────────
CREATE TABLE dbo.order_items (
  order_id BIGINT NOT NULL CONSTRAINT fk_items_order REFERENCES dbo.orders (id),
  line_no  INT NOT NULL,
  sku      NVARCHAR(64) NULL,
  qty      INT NULL,
  price    DECIMAL(10, 2) NULL,
  CONSTRAINT pk_order_items PRIMARY KEY (order_id, line_no)
);
GO

WITH seq AS (
  SELECT TOP (300) ROW_NUMBER() OVER (ORDER BY (SELECT NULL)) AS n FROM sys.all_objects
),
lines AS (SELECT 1 AS line_no UNION ALL SELECT 2 UNION ALL SELECT 3)
INSERT INTO dbo.order_items (order_id, line_no, sku, qty, price)
SELECT s.n, l.line_no, CONCAT('SKU-', s.n, '-', l.line_no), 1 + (s.n % 4),
       CAST((s.n * 17 % 10000) / 100.0 AS DECIMAL(10, 2))
FROM seq s
JOIN lines l ON l.line_no <= 1 + (s.n % 3);
GO

-- ── dbo.shipments: composite FK into order_items ───────────────────────────
CREATE TABLE dbo.shipments (
  id       INT IDENTITY(1,1) PRIMARY KEY,
  order_id BIGINT NULL,
  line_no  INT NULL,
  CONSTRAINT fk_shipments_item FOREIGN KEY (order_id, line_no)
    REFERENCES dbo.order_items (order_id, line_no)
);
GO

INSERT INTO dbo.shipments (order_id, line_no)
SELECT order_id, line_no FROM dbo.order_items WHERE line_no = 1;
GO

-- ── billing.invoices: a second cross-schema FK, into dbo.orders ────────────
CREATE TABLE billing.invoices (
  id       INT IDENTITY(1,1) PRIMARY KEY,
  order_id BIGINT NULL CONSTRAINT fk_invoices_order REFERENCES dbo.orders (id),
  amount   DECIMAL(18, 2) NULL,
  issued   DATE NULL
);
GO

INSERT INTO billing.invoices (order_id, amount, issued)
SELECT id, total, CAST(placed_at AS DATE) FROM dbo.orders;
GO

-- ── dbo.geo: the remaining scalar types, with a NULL in every column ───────
CREATE TABLE dbo.geo (
  id       INT PRIMARY KEY,
  doc      XML NULL,
  on_date  DATE NULL,
  at_time  TIME(3) NULL,
  stamp    DATETIME2(3) NULL,
  stamp_tz DATETIMEOFFSET(3) NULL,
  legacy   DATETIME NULL,
  ratio    DECIMAL(18, 6) NULL,
  token    UNIQUEIDENTIFIER NULL
);
GO

INSERT INTO dbo.geo (id, doc, on_date, at_time, stamp, stamp_tz, legacy, ratio, token) VALUES
  (1, CAST('<note>1</note>' AS XML), '2026-09-23', '13:45:07.250',
      '2026-09-23T13:45:07.250', '2026-09-23T13:45:07.250+05:30', '2026-09-23T13:45:07',
      -12.345678, '01923f4a-5b6c-7d8e-9f01-23456789abcd'),
  (2, CAST('<note>2</note>' AS XML), '1900-01-01', '00:00:00',
      '1900-01-01T00:00:00', '1900-01-01T00:00:00+00:00', '1900-01-01T00:00:00',
      0.000001, '00000000-0000-0000-0000-000000000000'),
  (3, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL);
GO

-- ── dbo.[Order ]]Log]: an identifier with an embedded `]`, which bracket
-- quoting has to double ─────────────────────────────────────────────────────
CREATE TABLE dbo.[Order ]]Log] (
  id  INT PRIMARY KEY,
  msg NVARCHAR(255) NULL
);
GO

INSERT INTO dbo.[Order ]]Log] (id, msg) VALUES (1, 'created'), (2, 'shipped'), (3, 'delivered');
GO

-- ── view ───────────────────────────────────────────────────────────────────
CREATE VIEW dbo.customer_orders AS
SELECT c.id AS customer_id, c.name, o.id AS order_id, o.total, o.placed_at
FROM dbo.customers c
JOIN dbo.orders o ON o.customer_id = c.id;
GO
