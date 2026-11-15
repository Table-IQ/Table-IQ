import { useEffect, useState } from "react";
import { getAiConfig, localLlmAvailable, saveAiConfig } from "../lib/commands";
import { logError } from "../lib/report";
import { StatusDot } from "./ui/StatusBar";
import { AI_MODELS, isAiProvider, type AiProvider } from "../lib/aiModels";

const field =
  "w-full rounded-md border border-border bg-bg px-2.5 py-1.5 text-sm text-text placeholder:text-muted focus:border-primary focus:outline-none";

/**
 * Body of the AI (BYOK) settings — provider, model, API key. Embedded as the
 * "ai" section of the unified Settings page (Settings.tsx); previously its
 * own drawer opened from the Query tab.
 */
export function AiSettingsForm() {
  const [provider, setProvider] = useState<AiProvider>("anthropic");
  const [model, setModel] = useState<string>(AI_MODELS.anthropic[0]);
  const [apiKey, setApiKey] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [ollamaUp, setOllamaUp] = useState<boolean | null>(null);

  useEffect(() => {
    getAiConfig()
      .then((c) => {
        if (!isAiProvider(c.provider)) {
          setError("The saved provider is unsupported. Choose a provider and model before saving.");
          return;
        }
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
    if (!model.trim() || saving) return;
    setSaving(true); setError(null); setStatus(null);
    try {
      await saveAiConfig(provider, model.trim(), apiKey.trim() || undefined);
      setStatus("Saved");
      setApiKey("");
    } catch {
      setError("Could not save AI settings. Check that your OS keychain is unlocked and try again.");
    } finally { setSaving(false); }
  };

  return (
    <div className="space-y-3 p-4">
      <label className="block text-xs text-muted">
        Provider
        <select
          className={field}
          value={provider}
          disabled={saving}
          onChange={(e) => {
            const next = e.target.value;
            if (!isAiProvider(next)) return;
            setProvider(next);
            setModel(AI_MODELS[next][0]);
            setApiKey(""); setStatus(null); setError(null);
          }}
        >
          <option value="anthropic">Anthropic</option>
          <option value="openai">OpenAI</option>
          <option value="gemini">Gemini</option>
          <option value="openrouter">OpenRouter</option>
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
        <input className={field} value={model} list="tiq-ai-models" spellCheck={false} disabled={saving} onChange={(e) => { setModel(e.target.value); setStatus(null); }} />
        <datalist id="tiq-ai-models">{AI_MODELS[provider].map((m) => <option key={m} value={m} />)}</datalist>
      </label>
      <p className="text-[11px] text-muted">Suggestions checked November 15, 2026. Choose one or enter any model ID available to your account. Saved models stay unchanged until you save.</p>
      {provider === "openrouter" && <p className="text-[11px] text-muted">Use an OpenRouter API key and a namespaced model ID. OpenRouter routes requests to the selected provider; its availability and data policy apply.</p>}
      {!isLocal && (
        <label className="block text-xs text-muted">
          API key (stored in OS keychain)
          <input
            className={field}
            type="password"
            value={apiKey}
            disabled={saving}
            onChange={(e) => { setApiKey(e.target.value); setStatus(null); }}
            placeholder="Leave blank to keep this provider’s saved key"
            autoComplete="off"
          />
        </label>
      )}
      <p className="text-[11px] leading-snug text-muted">
        Your key is stored in the OS keychain and authenticates requests to your chosen provider.
        AI sends your request and the metadata shown in "What gets sent", including SQL dialect
        and known keys. Explain, optimize and fix also send the query and relevant error text.
        Table rows are not added automatically. Generated SQL is inserted for you to review and run.
      </p>
      <div className="flex items-center gap-2 pt-1">
        {status && <span className="text-xs text-ok">{status}</span>}
        {error && <span role="alert" className="text-xs text-danger">{error}</span>}
        <span className="ml-auto" />
        <button
          onClick={save}
          disabled={saving || !model.trim()}
          className="rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-white hover:bg-primary-hover"
        >
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </div>
  );
}
