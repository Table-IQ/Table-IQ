# Provider model refresh for TableIQ

Observed November 15, 2026. Independent, read-only application review plus current official documentation and a live unauthenticated OpenRouter catalog fetch. No provider API key was available to this researcher; no paid inference or account-specific availability test was performed. Recommendations below are SQL-assistance evaluation candidates, not measured SQL-accuracy rankings.

## Recommended choices

Keep a curated balanced choice first in each provider menu, with a stronger reasoning option. Preserve a user's saved provider/model unless they deliberately change it. For the existing Anthropic-first product, the suggested new-install default is `claude-sonnet-5-5` **after** the response-parser/token-budget fixes below. This is an implementation recommendation, not a claim that one provider wins on TableIQ's workload.

| Provider | Balanced candidate | Higher-accuracy candidate | Other verified current choices |
| --- | --- | --- | --- |
| Google Gemini | `gemini-4-hydrogen` | `gemini-4-carbon` | `gemini-4-argon`; older stable `gemini-3.8-flash` |
| OpenAI | `gpt-6.1-sol` | `gpt-6-astra` | Keep throughput/visual models separate from the coding default. |
| Anthropic | `claude-sonnet-5-5` | `claude-opus-5-5` | `claude-fable-5-2` for a separately evaluated highest-capability option; `claude-haiku-5-5` for a cost/latency experiment. |
| OpenRouter | `anthropic/claude-sonnet-5.5` or the user's chosen verified family | `openai/gpt-6-astra` / `anthropic/claude-opus-5.5` / `google/gemini-4-carbon` | OpenRouter is a gateway: its IDs and availability must come from its catalog, not a provider-name concatenation rule. |

Google's current model cards explicitly classify Hydrogen, Argon and Carbon as **Stable**. Hydrogen targets efficient everyday coding; Carbon is its strongest reasoning candidate; Argon offers a frontier middle option. These are real current IDs, rather than names inferred from the date. [Hydrogen model card](https://aistudio.google.com/docs/models/gemini-4-hydrogen), [Carbon model card](https://aistudio.google.com/docs/models/gemini-4-carbon), [Argon model card](https://aistudio.google.com/docs/models/gemini-4-argon).

Official OpenAI documentation places Sol in the balanced coding role and Astra in the strongest reasoning role. Both exact IDs are documented. Anthropic lists Sonnet 5.5 for everyday coding and data analysis, Opus 5.5 for more demanding work, and Fable 5.2 for its highest-capability tier. None of these general descriptions establishes SQL correctness on TableIQ's schema/prompt. [GPT-6.1 Sol](https://developers.openai.com/api/docs/models/gpt-6.1-sol), [GPT-6 Astra](https://developers.openai.com/api/docs/models/gpt-6-astra), [Claude Sonnet 5.5](https://platform.claude.com/docs/en/models/sonnet-5-5/overview), [Claude Opus 5.5](https://platform.claude.com/docs/en/models/opus-5-5/overview), [Claude Fable 5.2](https://platform.claude.com/docs/en/models/fable-5-2/overview).

## Exact OpenRouter IDs observed live

A successful `GET https://openrouter.ai/api/v1/models` returned 470 records. Each ID below appeared verbatim, advertised text output, listed `/api/v1/chat/completions` among its endpoints, and had `expiration_date: null` at observation. This is catalog evidence, not a paid completion or an account-entitlement guarantee. [Live model catalog](https://openrouter.ai/api/v1/models), [catalog API documentation](https://openrouter.ai/docs/api/api-reference/models/list-all-models-and-their-properties).

```text
google/gemini-4-hydrogen
google/gemini-4-carbon
google/gemini-4-argon
google/gemini-3.8-flash
openai/gpt-6.1-sol
openai/gpt-6-astra
anthropic/claude-sonnet-5.5
anthropic/claude-opus-5.5
anthropic/claude-fable-5.2
```

Notice the punctuation difference: direct Claude uses `claude-sonnet-5-5`; the router uses `anthropic/claude-sonnet-5.5`. Do not substitute one for the other. Avoid batch suffixes, preview variants, free variants and moving latest aliases as the default for this interactive production path; select them only as explicit choices.

Catalog response SHA-256: `7d26cade96fe149969e4097ddf409919d7a8fa876bfa2aacf3392412d3e47743`. The original research snapshot is temporarily retained at `/tmp/tableiq-provider-catalog-20261115.json`; the catalog endpoint is the reproducible source.

## Existing architecture and compatibility

Application files inspected: `src/components/AiSettings.tsx`, `src-tauri/src/ai/mod.rs`, and the provider adapter `src-tauri/src/ai/provider.rs`. The app sends a schema/dialect/known-key context and asks for one SQL query; it does not give the model database tools. Its backend uses direct REST calls and currently reads a single first text field.

| Provider | Existing or required endpoint | Compatibility conclusion |
| --- | --- | --- |
| OpenAI | `POST https://api.openai.com/v1/chat/completions`; Bearer key | Sol/Astra document support for plain Chat Completions. Current TableIQ requests have no tools, so a Responses migration is not required just to refresh these IDs. |
| Gemini | `POST https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent`; `x-goog-api-key` | Google explicitly supports Hydrogen through GenerateContent as well as Interactions. The existing protocol remains a documented path; model stability and the `v1beta` endpoint name are separate matters. |
| Anthropic | `POST https://api.anthropic.com/v1/messages`; `x-api-key` and `anthropic-version` | The Messages protocol remains appropriate, but the existing parser and 1,024-token budget are not a safe string-only migration to Claude 5.5. |
| OpenRouter | Add an explicit branch for `POST https://openrouter.ai/api/v1/chat/completions`; Bearer OpenRouter key | Reuse the Chat Completions request/response shape with namespaced router IDs. It is currently absent from the form/provider match. The wildcard branch currently routes unknown providers to Anthropic, so adding only a provider string would be wrong. |

Sources: [OpenAI model endpoint table, explicit Markdown support column](https://developers.openai.com/api/docs/models/gpt-6.1-sol.md), [GPT-6 compatibility guide](https://developers.openai.com/api/docs/guides/latest-model), [Hydrogen API availability](https://aistudio.google.com/docs/whats-new-gemini-4-hydrogen), [Claude migration guide](https://platform.claude.com/docs/en/models/sonnet-5-5/migration-guide), [OpenRouter Chat Completions reference](https://openrouter.ai/docs/api/api-reference/chat/create-a-chat-completion).

### Migration details that affect correctness

- **Claude text extraction:** Sonnet 5.5 thinking is on when omitted. A response can begin with a `thinking` block, so `content[0].text` can fail. Collect blocks whose `type` is `text`. Revisit `max_tokens`: it covers reasoning plus final text. The current 1,024 cap can exhaust the response before useful SQL. An explicit, provider-aware budget and stop-reason check are required. Adaptive thinking with medium effort is a candidate for interactive testing, not a proven optimal SQL setting. [Official migration warning](https://platform.claude.com/docs/en/models/sonnet-5-5/migration-guide).
- **Claude thinking controls are model-specific:** Sonnet 5.5 rejects `thinking.type: disabled`. Its lowest setting is `between_tools` at high effort or below; without tools that produces text only. Opus 5.5/Fable thinking cannot be disabled. Do not apply one disabling switch to every Claude model. [Thinking API guide](https://platform.claude.com/docs/en/build-with-claude/thinking).
- **OpenAI reasoning parameters:** Sol/Astra do not support `none`; start at supported `low` or `medium` and evaluate. Do not add unsupported sampling parameters. Any future GPT-6 tool-calling path requires Responses; today's plain SQL-generation path does not. [Compatibility guidance](https://developers.openai.com/api/docs/guides/latest-model).
- **Gemini reasoning:** Hydrogen defaults to high and supports low/medium/high/xhigh; `minimal` is rejected. Test medium versus high rather than copying a legacy thinking budget. Do not copy Interactions field names into GenerateContent without its API schema. Filter final text parts, rather than assuming the first part is always the complete answer. [Hydrogen migration details](https://aistudio.google.com/docs/whats-new-gemini-4-hydrogen).
- **Carbon output cap:** Its migration guide advises apps with a limit below 32K to raise the combined thinking/output limit to at least 32,768 and lower the thinking level when reducing token use. Do not blindly apply one small output cap to every provider/model; measure cost and latency for this optional stronger-reasoning choice. [Carbon output-limit guidance](https://aistudio.google.com/docs/whats-new-gemini-4-carbon#setting-output-token-limits).
- **All adapters, local code finding:** Validate non-2xx responses, provider errors, empty final text, refusals and truncated stop reasons explicitly. Current missing-field-to-empty-string behavior can make an unavailable model or reasoning-only response look like an accuracy problem. Keep reasoning/progress fields out of inserted SQL.
- **Menu behavior, local code finding:** Provider changes select `MODELS[provider][0]`. Put the balanced entry first, rather than accidentally making the more expensive flagship the provider-switch default. Retain an explicit custom-ID or account-catalog path for existing saved models.

## Remove stale new-user recommendations

`o4-mini` shut down on **October 23, 2026**. GPT-4.1 and GPT-4o are deprecated with **January 14, 2027** shutdown dates listed for the current snapshots. They should not lead the new-user menu. Do not describe every GPT-4o snapshot as already shut down: the dates differ. [Official OpenAI deprecations](https://developers.openai.com/api/docs/deprecations).

Google's fresh catalog says `gemini-2.5-pro` and `gemini-2.5-flash` are deprecated, unavailable for new selection in AI Studio, and limited to existing API users. On **December 1, 2026**, Pro requests auto-route to `gemini-3.8-flash` and Flash requests to `gemini-3.5-flash-lite`. That differs from older cached pages saying 2.5 was not deprecated. This is a concrete reason to refresh the present Gemini choices. [Current catalog notices](https://aistudio.google.com/docs/models), [deprecation schedule](https://aistudio.google.com/docs/deprecations).

## Evidence limits and verification before calling this production-ready

The legacy `ai.google.dev` model URL now redirects to `aistudio.google.com`. Search snippets showed 3.8 Flash as latest; a Context7 index showed Argon alone; the freshly fetched canonical cards show all three stable Gemini 4 variants. Anthropic cached overview results similarly showed Fable 5.1, while the current overview shows 5.2. Prefer the fetched current card/catalog over snippets, date assumptions or local hard-coded names. OpenAI's extracted HTML lists unsupported endpoint names too; its Markdown table supplies the explicit Supported column.

No key was used to call a paid model. For the three direct providers, the actual account catalog and one small completion remain unverified. Use the user's selected account to list accessible models/capabilities, then perform a small generation before advertising connectivity: [OpenAI list models](https://developers.openai.com/api/reference/resources/models/methods/list), [Claude list models](https://platform.claude.com/docs/en/api/models/list), [Gemini models API](https://aistudio.google.com/api-reference/models). OpenRouter's public catalog is not a substitute for credits, provider availability or successful inference.

Before claiming improved SQL accuracy, compare balanced and stronger candidates on the same reviewed cases: dialect syntax, nullable joins, EXISTS versus duplicate-producing joins, composite keys, grouped totals, missing metadata and error repair. Judge returned results on a disposable synthetic database, response truncation, latency and cost. These checks select an actual TableIQ default; a generic coding claim does not. No application files were modified in this research task.
