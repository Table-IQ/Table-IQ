import { useEffect, useState } from "react";
import { getAiConfig, saveAiConfig } from "../lib/commands";

const MODELS: Record<string, string[]> = {
  anthropic: ["claude-opus-4-7", "claude-sonnet-4-6", "claude-haiku-4-5-20251001"],
  openai: ["gpt-4.1", "gpt-4o", "o4-mini"],
  gemini: ["gemini-2.5-pro", "gemini-2.5-flash"],
};

export function AiSettings({ onClose }: { onClose: () => void }) {
  const [provider, setProvider] = useState("anthropic");
  const [model, setModel] = useState("claude-sonnet-4-6");
  const [apiKey, setApiKey] = useState("");
  const [status, setStatus] = useState<string | null>(null);

  useEffect(() => {
    getAiConfig()
      .then((c) => {
        setProvider(c.provider);
        setModel(c.model);
      })
      .catch(() => {});
  }, []);

  const field =
    "w-full rounded-md border border-border bg-bg px-2.5 py-1.5 text-sm text-text placeholder:text-muted focus:border-primary focus:outline-none";

  const save = async () => {
    await saveAiConfig(provider, model, apiKey || undefined);
    setStatus("Saved");
    setApiKey("");
    setTimeout(onClose, 600);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={onClose}>
      <div
        className="w-96 rounded-xl border border-border bg-bg p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-3 text-sm font-semibold">AI settings (BYOK)</div>
        <div className="space-y-3">
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
            </select>
          </label>
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
          <p className="text-[11px] text-muted">
            Your key never leaves the keychain. NL→SQL sends only the schema card shown in “What gets
            sent” — never table data. Generated SQL is inserted for you to review and run.
          </p>
        </div>
        <div className="mt-4 flex items-center justify-end gap-2">
          {status && <span className="text-xs text-emerald-500">{status}</span>}
          <button onClick={onClose} className="rounded-md border border-border px-3 py-1.5 text-sm hover:bg-surface">
            Cancel
          </button>
          <button onClick={save} className="rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-white hover:bg-primary-hover">
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
