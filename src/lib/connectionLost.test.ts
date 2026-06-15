import { describe, expect, it } from "vitest";
import { isConnectionLostError } from "./commands";

// The banner must fire only when an *established* session drops — not on every
// query error. These strings are what sqlx / the mysql + mongo + redis drivers
// surface via `e.to_string()` when the server goes away (see the slice-30 connect
// probe in src-tauri/src/drivers/postgres.rs, which keys off the same phrases).
describe("isConnectionLostError", () => {
  it("matches real connection-loss messages across engines", () => {
    for (const msg of [
      "error returned from database: Connection refused (os error 61)",
      "pool timed out while waiting for an open connection",
      "server closed the connection unexpectedly",
      "Connection reset by peer (os error 54)",
      "io error: broken pipe",
      "terminating connection due to administrator command",
      "Lost connection to MySQL server during query",
      "MySQL server has gone away",
      "no connection to the server",
    ]) {
      expect(isConnectionLostError(msg), msg).toBe(true);
    }
  });

  it("does NOT match ordinary query/SQL errors (no false-positive banner)", () => {
    for (const msg of [
      'syntax error at or near "SELCT"',
      'relation "users" does not exist',
      "permission denied for table accounts",
      "duplicate key value violates unique constraint",
      "division by zero",
      "",
    ]) {
      expect(isConnectionLostError(msg), msg).toBe(false);
    }
  });
});
