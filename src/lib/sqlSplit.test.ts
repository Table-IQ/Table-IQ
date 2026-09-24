import { describe, expect, it } from "vitest";
import { lineColAt, splitStatements, statementAt } from "./sqlSplit";

describe("splitStatements", () => {
  it("splits simple statements on ; and trims whitespace", () => {
    const doc = "SELECT 1;\n  SELECT 2;  \nSELECT 3";
    const stmts = splitStatements(doc, "postgres");
    expect(stmts.map((s) => s.text)).toEqual(["SELECT 1", "SELECT 2", "SELECT 3"]);
    // from/to cover exactly `text`.
    for (const s of stmts) expect(doc.slice(s.from, s.to)).toBe(s.text);
  });

  it("does not split on ; inside a plain '' string, and handles '' doubling", () => {
    const doc = "SELECT 'it''s; a test' AS x; SELECT 2;";
    const stmts = splitStatements(doc, "postgres");
    expect(stmts.map((s) => s.text)).toEqual([
      "SELECT 'it''s; a test' AS x",
      "SELECT 2",
    ]);
  });

  it("does not split on ; inside a double-quoted identifier", () => {
    const doc = 'SELECT "weird;col" FROM t; SELECT 2;';
    const stmts = splitStatements(doc, "postgres");
    expect(stmts.map((s) => s.text)).toEqual(['SELECT "weird;col" FROM t', "SELECT 2"]);
  });

  it("honors PG E'' backslash escapes", () => {
    const doc = "SELECT E'a\\';b'; SELECT 2;";
    const stmts = splitStatements(doc, "postgres");
    // The backslash escapes the quote, so the first statement swallows the `;` too.
    expect(stmts).toHaveLength(2);
    expect(stmts[0].text).toBe("SELECT E'a\\';b'");
    expect(stmts[1].text).toBe("SELECT 2");
  });

  it("honors MySQL backslash escapes in '' and \"\" strings", () => {
    const doc = "SELECT 'a\\'b;c' FROM t; SELECT \"x\\\"y;z\" FROM u;";
    const stmts = splitStatements(doc, "mysql");
    expect(stmts.map((s) => s.text)).toEqual([
      "SELECT 'a\\'b;c' FROM t",
      'SELECT "x\\"y;z" FROM u',
    ]);
  });

  it("does not split on ; inside a MySQL backtick identifier", () => {
    const doc = "SELECT `weird;col` FROM t; SELECT 2;";
    const stmts = splitStatements(doc, "mysql");
    expect(stmts.map((s) => s.text)).toEqual(["SELECT `weird;col` FROM t", "SELECT 2"]);
  });

  it("does not split on ; inside a sqlserver [bracket] identifier", () => {
    const doc = "SELECT [weird;col] FROM t; SELECT 2;";
    const stmts = splitStatements(doc, "sqlserver");
    expect(stmts.map((s) => s.text)).toEqual(["SELECT [weird;col] FROM t", "SELECT 2"]);
  });

  it("does not split on ; inside a $$ function body", () => {
    const doc =
      "CREATE FUNCTION foo() RETURNS void AS $$\nBEGIN\n  SELECT 1;\nEND;\n$$ LANGUAGE plpgsql;\nSELECT 2;";
    const stmts = splitStatements(doc, "postgres");
    expect(stmts).toHaveLength(2);
    expect(stmts[0].text).toBe(
      "CREATE FUNCTION foo() RETURNS void AS $$\nBEGIN\n  SELECT 1;\nEND;\n$$ LANGUAGE plpgsql",
    );
    expect(stmts[1].text).toBe("SELECT 2");
  });

  it("does not split on ; inside a tagged $tag$ function body", () => {
    const doc = "SELECT $tag$a;b$tag$ AS x; SELECT 2;";
    const stmts = splitStatements(doc, "postgres");
    expect(stmts.map((s) => s.text)).toEqual(["SELECT $tag$a;b$tag$ AS x", "SELECT 2"]);
  });

  it("does not split on ; inside a -- line comment", () => {
    const doc = "SELECT 1; -- trailing ; comment\nSELECT 2;";
    const stmts = splitStatements(doc, "postgres");
    expect(stmts.map((s) => s.text)).toEqual([
      "SELECT 1",
      "-- trailing ; comment\nSELECT 2",
    ]);
  });

  it("does not split on ; inside a nested /* */ block comment", () => {
    const doc = "/* outer /* inner ; */ still outer ; */\nSELECT 1;";
    const stmts = splitStatements(doc, "postgres");
    expect(stmts).toHaveLength(1);
    expect(stmts[0].text).toBe("/* outer /* inner ; */ still outer ; */\nSELECT 1");
  });

  it("treats # as a line comment on MySQL (the rest of the line is not SQL)", () => {
    // MySQL ends a `#` comment at the newline, so the `;` and the DELETE after it
    // are COMMENT, not a second statement. Splitting here would run the DELETE.
    const doc = "SELECT count(*) FROM users # before cleanup; DELETE FROM users WHERE 1=1;";
    const stmts = splitStatements(doc, "mysql");
    expect(stmts.map((s) => s.text)).toEqual([doc]);
  });

  it("keeps a MySQL statement whole when its WHERE follows a # comment", () => {
    const doc = "UPDATE t SET a=1 # only ; for the demo\nWHERE id=5;";
    const stmts = splitStatements(doc, "mysql");
    expect(stmts.map((s) => s.text)).toEqual([
      "UPDATE t SET a=1 # only ; for the demo\nWHERE id=5",
    ]);
  });

  it("does not treat # as a comment for other dialects", () => {
    // `#` is an operator on Postgres (and nothing special on SQLite/SQL Server).
    const doc = "SELECT a # b; SELECT 2;";
    expect(splitStatements(doc, "postgres").map((s) => s.text)).toEqual([
      "SELECT a # b",
      "SELECT 2",
    ]);
    expect(splitStatements(doc, "sqlite").map((s) => s.text)).toEqual([
      "SELECT a # b",
      "SELECT 2",
    ]);
  });

  it("ends a MySQL block comment at the FIRST */ (MySQL does not nest)", () => {
    const doc = "SELECT 1; /* note /* about */\nDELETE FROM audit;";
    const stmts = splitStatements(doc, "mysql");
    expect(stmts.map((s) => s.text)).toEqual([
      "SELECT 1",
      "/* note /* about */\nDELETE FROM audit",
    ]);
  });

  it("ends a SQLite block comment at the FIRST */ (SQLite does not nest)", () => {
    const doc = "/* a /* b */\nDROP TABLE users;";
    const stmts = splitStatements(doc, "sqlite");
    expect(stmts.map((s) => s.text)).toEqual(["/* a /* b */\nDROP TABLE users"]);
  });

  it("nests block comments for sqlserver (T-SQL supports nested comments)", () => {
    // learn.microsoft.com/en-us/sql/t-sql/language-elements/slash-star-comment-transact-sql:
    // "Transact-SQL supports nested comments."
    const doc = "/* outer /* inner ; */ still outer ; */\nSELECT 1;";
    const stmts = splitStatements(doc, "sqlserver");
    expect(stmts).toHaveLength(1);
    expect(stmts[0].text).toBe("/* outer /* inner ; */ still outer ; */\nSELECT 1");
  });

  it("does not dollar-quote on MySQL/SQLite ($ is not a quote there)", () => {
    const doc = "SELECT a$b$c FROM t;\nDROP TABLE t;";
    for (const d of ["mysql", "sqlite", "sqlserver"] as const) {
      expect(splitStatements(doc, d).map((s) => s.text)).toEqual([
        "SELECT a$b$c FROM t",
        "DROP TABLE t",
      ]);
    }
  });

  it("drops comment-only and empty chunks", () => {
    const doc = "SELECT 1;\n-- just a trailing comment\n";
    const stmts = splitStatements(doc, "postgres");
    expect(stmts.map((s) => s.text)).toEqual(["SELECT 1"]);
  });

  it("drops empty chunks between consecutive ;;", () => {
    const doc = "SELECT 1;;SELECT 2;";
    const stmts = splitStatements(doc, "postgres");
    expect(stmts.map((s) => s.text)).toEqual(["SELECT 1", "SELECT 2"]);
  });

  it("returns no trailing empty statement after the final ;", () => {
    const doc = "SELECT 1; SELECT 2;";
    const stmts = splitStatements(doc, "postgres");
    expect(stmts).toHaveLength(2);
  });

  it("returns the last statement even without a trailing ;", () => {
    const doc = "SELECT 1;\nSELECT 2";
    const stmts = splitStatements(doc, "postgres");
    expect(stmts.map((s) => s.text)).toEqual(["SELECT 1", "SELECT 2"]);
  });

  it("splits on a line containing only GO for sqlserver", () => {
    const doc = "SELECT 1\nGO\nSELECT 2\ngo\nSELECT 3";
    const stmts = splitStatements(doc, "sqlserver");
    expect(stmts.map((s) => s.text)).toEqual(["SELECT 1", "SELECT 2", "SELECT 3"]);
  });

  it("does not treat GO as a separator for other dialects", () => {
    const doc = "SELECT 1\nGO\nSELECT 2";
    const stmts = splitStatements(doc, "postgres");
    expect(stmts).toHaveLength(1);
    expect(stmts[0].text).toBe("SELECT 1\nGO\nSELECT 2");
  });

  it("does not treat a line with more than GO as a separator", () => {
    const doc = "SELECT 1\nSELECT GO_TABLE;\nGO\nSELECT 2;";
    const stmts = splitStatements(doc, "sqlserver");
    expect(stmts.map((s) => s.text)).toEqual(["SELECT 1\nSELECT GO_TABLE", "SELECT 2"]);
  });

  it("returns an empty array for a blank document", () => {
    expect(splitStatements("", "postgres")).toEqual([]);
    expect(splitStatements("   \n  ", "postgres")).toEqual([]);
  });
});

describe("statementAt", () => {
  const doc = "SELECT 1;\nSELECT 2;\nSELECT 3;";
  const stmts = splitStatements(doc, "postgres");

  it("returns undefined for an empty statement list", () => {
    expect(statementAt([], 5)).toBeUndefined();
  });

  it("returns the first statement when the cursor is before it", () => {
    expect(statementAt(stmts, 0)?.text).toBe("SELECT 1");
  });

  it("returns the statement containing the cursor", () => {
    expect(statementAt(stmts, 15)?.text).toBe("SELECT 2");
  });

  it("returns the nearest previous statement right after the final ;", () => {
    // Cursor placed exactly after the doc's last `;` (end of doc).
    expect(statementAt(stmts, doc.length)?.text).toBe("SELECT 3");
  });

  it("returns the nearest previous statement when in trailing whitespace", () => {
    // Cursor in the blank gap between two statements (not yet touching stmt 2's text).
    const d = "SELECT 1;\n\n   \nSELECT 2;";
    const s = splitStatements(d, "postgres");
    const gapPos = d.indexOf("   ") + 1;
    expect(statementAt(s, gapPos)?.text).toBe("SELECT 1");
  });

  it("returns the last statement when in a trailing comment at the end of the doc", () => {
    // A comment with nothing after it is dropped as its own chunk, so the cursor
    // there still resolves to the last real statement.
    const d = "SELECT 1;\n-- trailing comment, nothing follows";
    const s = splitStatements(d, "postgres");
    expect(s).toHaveLength(1);
    const commentPos = d.indexOf("trailing");
    expect(statementAt(s, commentPos)?.text).toBe("SELECT 1");
  });
});

describe("lineColAt", () => {
  it("is 1-based at the start of the document", () => {
    expect(lineColAt("SELECT 1", 0)).toEqual({ line: 1, col: 1 });
  });

  it("tracks line and column across newlines", () => {
    const doc = "SELECT 1;\nSELECT 2;";
    expect(lineColAt(doc, 10)).toEqual({ line: 2, col: 1 });
    expect(lineColAt(doc, 13)).toEqual({ line: 2, col: 4 });
  });
});
