import { describe, expect, it } from "vitest";
import { escapeHtml, highlightSql } from "./SqlLogPanel";

describe("escapeHtml", () => {
  it("escapes &, < and >", () => {
    expect(escapeHtml("a & b < c > d")).toBe("a &amp; b &lt; c &gt; d");
  });

  it("leaves quotes untouched — output only ever lands in HTML text content, never an attribute", () => {
    expect(escapeHtml(`it's "quoted"`)).toBe(`it's "quoted"`);
  });

  it("leaves plain text untouched", () => {
    expect(escapeHtml("SELECT 1")).toBe("SELECT 1");
  });
});

describe("highlightSql", () => {
  it("wraps a recognized token in a span with a highlight class", () => {
    const html = highlightSql("SELECT 1");
    expect(html).toContain("<span");
    expect(html).toContain("SELECT");
  });

  it("HTML-escapes a literal containing markup so the log can never inject markup", () => {
    const html = highlightSql(`SELECT '<img src=x onerror=alert(1)>'`);
    expect(html).not.toContain("<img");
    expect(html).toContain("&lt;img");
  });

  it("escapes & and > the same way inside a literal", () => {
    const html = highlightSql(`SELECT 'a & b > c'`);
    expect(html).toContain("a &amp; b &gt; c");
  });

  it("does not throw on malformed/incomplete SQL — lezer's error recovery still yields tokens", () => {
    expect(() => highlightSql("SELECT * FROM WHERE")).not.toThrow();
  });

  it("does not throw and returns a string for empty input", () => {
    expect(highlightSql("")).toBe("");
  });

  it("round-trips a redacted password statement (the exact shape the SQL log stores)", () => {
    const html = highlightSql(`ALTER ROLE "bob" PASSWORD '***';`);
    expect(html).toContain("bob");
    expect(html).toContain("***");
    expect(html).not.toContain("<script");
  });
});
