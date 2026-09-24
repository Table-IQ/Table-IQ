import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { logClientError } from "./lib/commands";
import { logError } from "./lib/report";
import "./index.css";

// A webview crash otherwise leaves no trace at all: the console dies with the window.
window.addEventListener("error", (e) =>
  logError(`window error (${e.filename}:${e.lineno})`, e.error ?? e.message),
);
window.addEventListener("unhandledrejection", (e) => logError("unhandled rejection", e.reason));
// A CSP that blocks something the UI needs is otherwise silent outside devtools.
window.addEventListener("securitypolicyviolation", (e) =>
  logClientError(`csp blocked ${e.violatedDirective}: ${e.blockedURI} (${e.sourceFile}:${e.lineNumber})`),
);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
