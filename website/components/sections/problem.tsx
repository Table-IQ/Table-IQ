import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";

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
    </SectionShell>
  );
}
