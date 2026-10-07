-- Seed for tests/clickhouse_live.rs. Runs once, on the image's first start (`down -v` resets).
-- `tableiq` already exists (CLICKHOUSE_DB). Every column type the driver has to render is here.
CREATE DATABASE IF NOT EXISTS tableiq_aux;

CREATE TABLE tableiq.types
(
    id        UInt64,
    u64_max   UInt64,
    i64_min   Int64,
    i32       Int32,
    f64       Float64,
    f32       Float32,
    dec       Decimal(18, 4),
    dec_big   Decimal(38, 10),
    s         String,
    ts        DateTime('UTC'),
    ts64      DateTime64(3, 'UTC'),
    d         Date,
    ns        Nullable(String),
    ni        Nullable(Int32),
    arr       Array(String),
    lc        LowCardinality(String),
    u         UUID,
    e         Enum8('red' = 1, 'green' = 2),
    flag      Bool,
    m         Map(String, UInt32),
    t         Tuple(UInt8, String)
)
ENGINE = MergeTree ORDER BY id COMMENT 'one row of every type';

INSERT INTO tableiq.types VALUES
(1, 18446744073709551615, -9223372036854775808, -2147483648, 0.1, 1.5, 12345678.9012, 1234567890123456789012345678.0123456789,
 'héllo, wörld', '2024-03-05 06:07:08', '2024-03-05 06:07:08.123', '2024-03-05', 'present', 42, ['a', 'b,c', 'it''s'], 'low',
 '123e4567-e89b-12d3-a456-426614174000', 'green', true, map('one', 1, 'two', 2), (7, 'seven')),
(2, 0, 0, 0, 0, 0, 0, 0, '', '1970-01-01 00:00:00', '1970-01-01 00:00:00.000', '1970-01-01', NULL, NULL, [], '', '00000000-0000-0000-0000-000000000000', 'red', false, map(), (0, ''));

-- Two key columns; the sorting key (and so the "primary key") is (region, id).
CREATE TABLE tableiq.orders
(
    region String,
    id     UInt32,
    total  Decimal(10, 2),
    note   Nullable(String)
)
ENGINE = MergeTree ORDER BY (region, id);

INSERT INTO tableiq.orders SELECT ['north', 'south', 'east'][number % 3 + 1], intDiv(number, 3), number * 37 % 100, if(number % 4 = 0, NULL, concat('note ', toString(number))) FROM numbers(30);

CREATE VIEW tableiq.big_orders AS SELECT region, id, total FROM tableiq.orders WHERE total > 50;

-- ~200k rows, past the grid's page size and under EXACT_COUNT_MAX.
CREATE TABLE tableiq.events
(
    id      UInt64,
    kind    LowCardinality(String),
    ts      DateTime('UTC'),
    amount  Float64,
    payload String
)
ENGINE = MergeTree ORDER BY id;

CREATE TABLE tableiq_aux.notes (id UInt8, body String) ENGINE = MergeTree ORDER BY id;
INSERT INTO tableiq_aux.notes VALUES (1, 'in the second database');

-- The typical analytics login: a profile with readonly = 1, which refuses any per-request setting.
CREATE USER tiq_ro IDENTIFIED BY 'tableiq' SETTINGS readonly = 1;
GRANT SELECT ON tableiq.* TO tiq_ro;
GRANT SELECT ON system.* TO tiq_ro;
GRANT SELECT ON tableiq_aux.* TO tiq_ro;

-- Last: the healthcheck waits for exactly this many rows.
INSERT INTO tableiq.events
SELECT number, ['click', 'view', 'buy', 'scroll'][number % 4 + 1], toDateTime('2024-01-01 00:00:00', 'UTC') + number, number / 8, concat('event-', toString(number))
FROM numbers(200000);
