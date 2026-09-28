import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ErrorBoundary, ErrorFallback } from "./ErrorBoundary";

// No jsdom in this repo (vitest runs in node), so these render through
// react-dom/server. React does not run error boundaries during server rendering,
// hence the fallback is exercised directly and the catching contract through
// `getDerivedStateFromError`, which is a pure static method.
describe("ErrorBoundary", () => {
  it("renders its children untouched when nothing throws", () => {
    const html = renderToStaticMarkup(
      <ErrorBoundary>
        <span>the grid</span>
      </ErrorBoundary>,
    );
    expect(html).toBe("<span>the grid</span>");
  });

  it("turns a thrown value into the panel's message", () => {
    expect(ErrorBoundary.getDerivedStateFromError(new Error("cannot read 'rows'"))).toEqual({
      message: "cannot read 'rows'",
    });
    expect(ErrorBoundary.getDerivedStateFromError("plain string")).toEqual({
      message: "plain string",
    });
  });
});

describe("ErrorFallback", () => {
  it("shows the headline, the message and a way back", () => {
    const html = renderToStaticMarkup(<ErrorFallback message="cannot read 'rows'" />);
    expect(html).toContain("Something went wrong");
    expect(html).toContain("cannot read &#x27;rows&#x27;");
    expect(html).toContain("Reload");
  });
});
