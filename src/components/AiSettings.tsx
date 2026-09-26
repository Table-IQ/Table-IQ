import { useEffect, useState } from "react";
import { getAiConfig, localLlmAvailable, saveAiConfig } from "../lib/commands";
import { logError } from "../lib/report";
import { StatusDot } from "./ui/StatusBar";

const MODELS: Record<string, string[]> = {
  anthropic: ["claude-opus-4-7", "claude-sonnet-4-6", "claude-haiku-4-5-20251001"],
  openai: ["gpt-4.1", "gpt-4o", "o4-mini"],
  gemini: ["gemini-2.5-pro", "gemini-2.5-flash"],
  ollama: ["qwen2.5-coder:7b", "qwen2.5-coder:1.5b", "llama3.1:8b"],
};

const field =
  "w-full rounded-md border border-border bg-bg px-2.5 py-1.5 text-sm text-text placeholder:text-muted focus:border-primary focus:outline-none";

/**
 * Body of the AI (BYOK) settings — provider, model, API key. Embedded as the
 * "ai" section of the unified Settings page (Settings.tsx); previously its
 * own drawer opened from the Query tab.
 */
export function AiSettingsForm() {
  const [provider, setProvider] = useState("anthropic");
  const [model, setModel] = useState("claude-sonnet-4-6");
  const [apiKey, setApiKey] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [ollamaUp, setOllamaUp] = useState<boolean | null>(null);

  useEffect(() => {
    getAiConfig()
      .then((c) => {
        setProvider(c.provider);
        setModel(c.model);
      })
      // ponytail: the panel's status slot is a green "Saved" indicator only, so a load
      // failure goes to the log and the form shows defaults. Upgrade: make status tri-state.
      .catch((e) => logError("ai settings: load config", e));
    localLlmAvailable().then(setOllamaUp).catch(() => setOllamaUp(false));
  }, []);

  const isLocal = provider === "ollama";

  const save = async () => {
    await saveAiConfig(provider, model, apiKey || undefined);
    setStatus("Saved");
    setApiKey("");
  };

  return (
    <div className="space-y-3 p-4">
      <label className="block text-xs text-muted">
        Provider
        <select
          className={field}
          value={provider}
          onChange={(e) => {
            setProvider(e.target.value);
            setModel(MODELS[e.target.value][0]);
          }}
        >
          <option value="anthropic">Anthropic</option>
          <option value="openai">OpenAI</option>
          <option value="gemini">Gemini</option>
          <option value="ollama">Ollama (local · zero egress)</option>
        </select>
      </label>
      {isLocal && (
        <div className="flex items-center gap-1.5 text-[11px]">
          <StatusDot tone={ollamaUp ? "ok" : "warn"} />
          <span className="text-muted">
            {ollamaUp === null
              ? "Checking local Ollama…"
              : ollamaUp
              ? "Ollama detected — fully local, nothing leaves your machine."
              : "Ollama not detected. Install it and run `ollama serve`."}
          </span>
        </div>
      )}
      <label className="block text-xs text-muted">
        Model
        <select className={field} value={model} onChange={(e) => setModel(e.target.value)}>
          {MODELS[provider].map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
      </label>
      {!isLocal && (
        <label className="block text-xs text-muted">
          API key (stored in OS keychain)
          <input
            className={field}
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="sk-…"
          />
        </label>
      )}
      <p className="text-[11px] leading-snug text-muted">
        Your key never leaves the keychain. NL→SQL sends only the schema card shown in
        "What gets sent" — never table data. Generated SQL is inserted for you to review
        and run.
      </p>
      <div className="flex items-center gap-2 pt-1">
        {status && <span className="text-xs text-ok">{status}</span>}
        <span className="ml-auto" />
        <button
          onClick={save}
          className="rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-white hover:bg-primary-hover"
        >
          Save
        </button>
      </div>
    </div>
  );
}
