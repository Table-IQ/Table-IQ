/** Curated suggestions verified against provider docs/catalogs on 2026-11-15.
 * Keep saved/custom IDs: account access and model lifetimes vary. */
export const AI_MODELS = {
  anthropic: ["claude-sonnet-5-5", "claude-opus-5-5", "claude-haiku-5-5", "claude-fable-5-2"],
  openai: ["gpt-6.1-sol", "gpt-6-astra"],
  gemini: ["gemini-4-hydrogen", "gemini-4-carbon", "gemini-4-argon", "gemini-3.8-flash"],
  openrouter: ["anthropic/claude-sonnet-5.5", "openai/gpt-6.1-sol", "openai/gpt-6-astra", "google/gemini-4-hydrogen", "google/gemini-4-carbon", "google/gemini-4-argon", "anthropic/claude-opus-5.5", "anthropic/claude-fable-5.2"],
  ollama: ["qwen2.5-coder:7b", "qwen2.5-coder:1.5b", "llama3.1:8b"],
} as const;
export type AiProvider = keyof typeof AI_MODELS;
export function isAiProvider(value: string): value is AiProvider {
  return Object.prototype.hasOwnProperty.call(AI_MODELS, value);
}
