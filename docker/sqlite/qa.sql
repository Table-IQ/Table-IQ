-- Task 2 (prod-ready): QA seed for SQLite. Idempotent: drops+recreates its
-- own objects only. Build: sqlite3 /tmp/tiq-qa.sqlite < docker/sqlite/qa.sql

PRAGMA foreign_keys = ON;

DROP VIEW IF EXISTS child_view;
DROP TRIGGER IF EXISTS trg_child_insert;
DROP TABLE IF EXISTS child;
DROP TABLE IF EXISTS parent;
DROP TABLE IF EXISTS child_log;

CREATE TABLE parent (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL
);

INSERT INTO parent (id, name) VALUES
  (1, 'alpha'),
  (2, 'beta'),
  (3, 'gamma');

-- Implicit FK target (no column list, `REFERENCES parent`): SQLite's
-- `PRAGMA foreign_key_list` returns NULL for the "to" column in this case.
-- SqliteDriver::foreign_keys() reads it via `r.get("to")` into a non-Option
-- String, which panics on that NULL — this is the Task 2 baseline probe.
CREATE TABLE child (
  id INTEGER PRIMARY KEY,
  parent_id INTEGER REFERENCES parent,
  blob_data BLOB,
  created_at TEXT DEFAULT (datetime('now'))
);

INSERT INTO child (id, parent_id, blob_data) VALUES
  (1, 1, X'DEADBEEF'),
  (2, 2, X'00FF00FF'),
  (3, NULL, NULL);

CREATE INDEX child_parent_idx ON child (parent_id);

CREATE VIEW child_view AS
SELECT child.id, child.parent_id, parent.name AS parent_name
FROM child
JOIN parent ON parent.id = child.parent_id;

CREATE TABLE child_log (
  id INTEGER PRIMARY KEY,
  child_id INTEGER,
  action TEXT
);

CREATE TRIGGER trg_child_insert
AFTER INSERT ON child
BEGIN
  INSERT INTO child_log (child_id, action) VALUES (NEW.id, 'inserted');
END;
