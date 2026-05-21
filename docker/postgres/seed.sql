-- Phase 0 cursor-path dataset: a wide table matching the spec's tricky-type shape.
-- 1,000,000 rows × 100+ columns, including NUMERIC(38,10), large TEXT, NULL-heavy, timestamptz.

CREATE TABLE wide (
  id             bigint PRIMARY KEY,
  amount_numeric numeric(38, 10) NOT NULL,
  created_at     timestamptz NOT NULL,
  blob_text      text NOT NULL,
  nullable_int   bigint
);

-- Add c6..c101 plain bigint columns to reach 100+ columns before the bulk insert.
DO $$
BEGIN
  FOR i IN 6..101 LOOP
    EXECUTE format('ALTER TABLE wide ADD COLUMN c%s bigint NOT NULL DEFAULT 0', i);
  END LOOP;
END $$;

INSERT INTO wide (id, amount_numeric, created_at, blob_text, nullable_int)
SELECT
  g,
  (g * 1234.5)::numeric(38, 10),
  timestamptz '2020-09-13 12:26:40+00' + (g || ' minutes')::interval,
  g || '-' || repeat('x', 1024),
  CASE WHEN g % 3 = 0 THEN NULL ELSE g END
FROM generate_series(1, 1000000) AS g;

ANALYZE wide;
