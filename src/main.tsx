import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { logClientError } from "./lib/commands";
import "./index.css";

// A webview crash otherwise leaves no trace at all: the console dies with the window.
window.addEventListener("error", (e) => logClientError(`${e.message} (${e.filename}:${e.lineno})`));
window.addEventListener("unhandledrejection", (e) => logClientError(`unhandled rejection: ${String(e.reason)}`));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
