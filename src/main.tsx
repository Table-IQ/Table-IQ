import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { logClientError } from "./lib/commands";
import "./index.css";

// A webview crash otherwise leaves no trace at all: the console dies with the window.
window.addEventListener("error", (e) => logClientError(`${e.message} (${e.filename}:${e.lineno})`));
window.addEventListener("unhandledrejection", (e) => logClientError(`unhandled rejection: ${String(e.reason)}`));
// A CSP that blocks something the UI needs is otherwise silent outside devtools.
window.addEventListener("securitypolicyviolation", (e) =>
  logClientError(`csp blocked ${e.violatedDirective}: ${e.blockedURI} (${e.sourceFile}:${e.lineNumber})`),
);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
