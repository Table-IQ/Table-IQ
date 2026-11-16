import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";
import { GITHUB } from "@/lib/site";

export function Problem() {
  return (
    <SectionShell id="ai" surface="paper-2" label="problem-title">
      <SectionHeading id="problem-title" title="Optional AI. Your provider. Your key.">
        Use the database client on its own, or configure a provider for help with SQL. TableIQ does not sell AI credits or require an AI subscription.
      </SectionHeading>
      <div className="grid gap-8 md:grid-cols-2">
        <div className="rounded-card border border-rule bg-paper p-6 md:p-8">
          <h3 className="text-title font-medium">Configure it in Settings → AI</h3>
          <ol className="mt-4 list-decimal space-y-3 pl-5 text-ink-2">
            <li>Choose Anthropic, OpenAI, Gemini or OpenRouter and a supported model.</li>
            <li>Add an API key from that provider. Keys are stored in the macOS Keychain.</li>
            <li>Or select Ollama to use a model served locally on your Mac.</li>
          </ol>
        </div>
        <div className="space-y-5 py-2 text-ink-2">
          <div><h3 className="text-title font-medium text-ink">Keep control of cost and context</h3><p className="mt-2">Provider API usage is billed separately. A consumer chat subscription may not include API access. When you use AI, context goes to the provider you configure; its data handling rules apply.</p></div>
          <div><h3 className="text-title font-medium text-ink">Check the SQL yourself</h3><p className="mt-2">AI assistance is preview functionality; provider and model compatibility varies. Review any suggested SQL before running it, and check the results. AI suggestions are not a correctness guarantee.</p></div>
        </div>
      </div>
      <figure className="mt-10">
        <img src="/img/ai-native-0817.png" width={2640} height={1640} loading="lazy" className="h-auto w-full rounded-card border border-rule" alt="Native TableIQ 0.8.17 showing AI-generated SQL and two manually verified result rows for paid orders without shipped shipments" />
        <figcaption className="mt-4 max-w-3xl text-sm text-muted">
          A real native example: Gemini 4 Hydrogen generated this SQL, then a manual Run returned the expected two orders in a synthetic SQLite database. One verified example; provider and model results vary. <a href={`${GITHUB}/blob/main/docs/launch/research/2026-11-16-native-ai-acceptance.md`} className="underline decoration-rule underline-offset-4 hover:text-ink">Read the test and its limits.</a>
        </figcaption>
      </figure>
    </SectionShell>
  );
}
