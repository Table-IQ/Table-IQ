# Live Gemini SQL check — November 15, 2026

User-authorized API key, retained outside source and release assets. Model: `gemini-4-hydrogen`, native GenerateContent contract with MEDIUM thinking, 32,768 combined-output cap. This pre-build semantic probe used the application's generation system prompt and equivalent schema/foreign-key metadata, not the complete native UI. No real database information or row values were sent; the fixture was fictional SQLite storefront metadata. The local verification copy added a customer without orders and a duplicate paid-payment record.

| Case | Observed result | Task outcome |
| --- | --- | --- |
| Paid orders not shipped, without explicit status semantics | Returned a SQL comment requesting the meaning of status values; no executable answer | Correctly exposed ambiguity; task not completed |
| Customer counts including zero-order customers | Request timed out after 100 seconds | No result; not a correctness pass |
| Highest paid-order total with duplicate payment records | EXISTS query counted each order once; Acme Demo, 5,600 cents | Passed this fixture |
| Refund reasons/amounts absent from schema | Returned a missing-schema comment instead of inventing refunds | Passed missing-information behavior |

Two of four target outcomes completed as specified. This small sample is not an accuracy rate for TableIQ or a model comparison. The first outcome is a clarification rather than incorrect SQL; it shows why schema-only AI needs business definitions. Successful replies took approximately 30–80 seconds on this account/path. Tiny `gemini-3.8-flash` transport probes also timed out at 20/25 seconds. No inference speed or reliability claim follows from model-card positioning.

The native walkthrough will define the payment/shipment status literals in its natural-language question, then show actual generated SQL, manual review/Run and returned rows. Condensing the wait in a film must be disclosed. No response is fabricated or replaced with a canned answer.

OpenAI, Claude and OpenRouter IDs/protocols were verified from current primary documentation/catalogs and parser regressions. No key for those services was supplied; live inference for them is not claimed.

## Native verification status

The supplied key still authenticated direct Google catalog requests, but the native saved-key request returned HTTP 401 (invalid API key). The cause is unproven: a safe equality check of the Keychain entry requires the owner to allow the macOS prompt. After installing 0.8.16, Orca reported Accessibility access blocked despite permissions showing granted. Native AI generation, manual execution and its success screenshot remain pending those operating-system access steps. Failed recordings are not release assets. The published films demonstrate actual external-agent MCP and manual SQL workflows, not built-in AI success.
