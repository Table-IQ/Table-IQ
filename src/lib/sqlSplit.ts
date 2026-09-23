/**
 * Split a SQL document into individual statements on top-level `;` (or, for
 * sqlserver, a line containing only `GO`), while ignoring separators inside
 * string/identifier literals, comments, and PG dollar-quoted bodies.
 *
 * Pure — no CodeMirror imports — so it tests cleanly in Vitest. Used by
 * `sqlSelection.ts` to know what "the statement under the cursor" means, and
 * by the Query tab (Task 6) to run/step through multiple statements.
 */
export type SqlDialect = "postgres" | "mysql" | "sqlite" | "sqlserver";

export interface SqlStatement {
  text: string;
  /** Doc-coords [from, to) covering `text` exactly: `doc.slice(from, to) === text`. */
  from: number;
  to: number;
}

type State =
  | "default"
  | "single"
  | "double"
  | "backtick"
  | "bracket"
  | "dollar"
  | "linecomment"
  | "blockcomment";

const WS = /\s/;

export function splitStatements(doc: string, dialect: SqlDialect): SqlStatement[] {
  const n = doc.length;
  const result: SqlStatement[] = [];

  let state: State = "default";
  let segStart = 0;
  let hasCode = false;
  let lineStart = 0;
  let blockDepth = 0;
  let dollarTag = "";
  // Whether the currently-open '/"" string honors backslash escapes.
  let backslashOn = false;

  function endSegment(endRaw: number, newStart: number): void {
    if (hasCode) {
      let f = segStart;
      let t = endRaw;
      while (f < t && WS.test(doc[f])) f++;
      while (t > f && WS.test(doc[t - 1])) t--;
      if (f < t) result.push({ text: doc.slice(f, t), from: f, to: t });
    }
    segStart = newStart;
    hasCode = false;
  }

  function maybeGoLine(start: number, end: number): void {
    if (dialect !== "sqlserver") return;
    const line = doc.slice(start, end);
    if (line.trim().length > 0 && /^\s*GO\s*$/i.test(line)) {
      endSegment(start, end + 1);
    }
  }

  let i = 0;
  while (i < n) {
    const c = doc[i];

    if (c === "\n") {
      if (state === "default") maybeGoLine(lineStart, i);
      else if (state === "linecomment") state = "default";
      lineStart = i + 1;
      i++;
      continue;
    }

    switch (state) {
      case "default": {
        if (c === ";") {
          endSegment(i, i + 1);
          i++;
          continue;
        }
        if (c === "-" && doc[i + 1] === "-") {
          state = "linecomment";
          i += 2;
          continue;
        }
        if (c === "/" && doc[i + 1] === "*") {
          state = "blockcomment";
          blockDepth = 1;
          i += 2;
          continue;
        }
        if (c === "'") {
          const prev = doc[i - 1];
          const prev2 = doc[i - 2];
          const extended =
            dialect === "postgres" &&
            (prev === "E" || prev === "e") &&
            (i - 2 < 0 || !/[A-Za-z0-9_$]/.test(prev2));
          backslashOn = dialect === "mysql" || extended;
          state = "single";
          hasCode = true;
          i++;
          continue;
        }
        if (c === '"') {
          backslashOn = dialect === "mysql";
          state = "double";
          hasCode = true;
          i++;
          continue;
        }
        if (dialect === "mysql" && c === "`") {
          state = "backtick";
          hasCode = true;
          i++;
          continue;
        }
        if (dialect === "sqlserver" && c === "[") {
          state = "bracket";
          hasCode = true;
          i++;
          continue;
        }
        if (c === "$") {
          const m = /^\$([A-Za-z_][A-Za-z0-9_]*)?\$/.exec(doc.slice(i));
          if (m) {
            dollarTag = m[0];
            state = "dollar";
            hasCode = true;
            i += dollarTag.length;
            continue;
          }
        }
        if (!WS.test(c)) hasCode = true;
        i++;
        continue;
      }
      case "single":
      case "double": {
        const quote = state === "single" ? "'" : '"';
        if (backslashOn && c === "\\") {
          i += 2; // skip the escaped char too
          continue;
        }
        if (c === quote) {
          if (doc[i + 1] === quote) {
            i += 2; // doubled-quote escape
            continue;
          }
          state = "default";
        }
        i++;
        continue;
      }
      case "backtick": {
        if (c === "`") {
          if (doc[i + 1] === "`") {
            i += 2;
            continue;
          }
          state = "default";
        }
        i++;
        continue;
      }
      case "bracket": {
        if (c === "]") {
          if (doc[i + 1] === "]") {
            i += 2;
            continue;
          }
          state = "default";
        }
        i++;
        continue;
      }
      case "dollar": {
        if (doc.startsWith(dollarTag, i)) {
          i += dollarTag.length;
          state = "default";
          continue;
        }
        i++;
        continue;
      }
      case "linecomment": {
        // \n is handled above (exits the comment); anything else is just skipped.
        i++;
        continue;
      }
      case "blockcomment": {
        if (c === "/" && doc[i + 1] === "*") {
          blockDepth++;
          i += 2;
          continue;
        }
        if (c === "*" && doc[i + 1] === "/") {
          blockDepth--;
          i += 2;
          if (blockDepth === 0) state = "default";
          continue;
        }
        i++;
        continue;
      }
    }
  }

  if (state === "default") maybeGoLine(lineStart, n);
  endSegment(n, n);

  return result;
}

/**
 * The statement `pos` (a doc offset) belongs to: the one containing it, or —
 * if `pos` is after a statement's end (past its `;`, or in trailing
 * whitespace/comments) — the nearest preceding statement. Before the first
 * statement, returns the first. Empty input returns `undefined`.
 */
export function statementAt(
  stmts: SqlStatement[],
  pos: number,
): SqlStatement | undefined {
  if (stmts.length === 0) return undefined;
  let result = stmts[0];
  for (const s of stmts) {
    if (s.from <= pos) result = s;
    else break;
  }
  return result;
}

/** 1-based {line, col} for a doc offset. */
export function lineColAt(doc: string, offset: number): { line: number; col: number } {
  let line = 1;
  let col = 1;
  const end = Math.min(offset, doc.length);
  for (let i = 0; i < end; i++) {
    if (doc[i] === "\n") {
      line++;
      col = 1;
    } else {
      col++;
    }
  }
  return { line, col };
}
