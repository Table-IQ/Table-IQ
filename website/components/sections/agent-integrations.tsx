import { AgentTrace } from "@/components/motion/agent-trace";
import { AdvancedDemos } from "@/components/sections/demo";
import { Guides } from "@/components/sections/guides";
import { Proof } from "@/components/sections/proof";
import { GITHUB } from "@/lib/site";

export function AgentIntegrations() {
  return (
    <section id="agents" className="scroll-mt-16 border-y border-rule bg-paper-2" aria-label="Optional coding-agent integrations">
      <details className="group">
        <summary className="mx-auto flex max-w-site cursor-pointer list-none items-center justify-between gap-6 px-gutter py-10 marker:content-none [&::-webkit-details-marker]:hidden">
          <span><span className="block text-title font-medium">Optional: connect a coding agent</span><span className="mt-2 block text-ink-2">MCP setup, activity previews and advanced demonstrations.</span></span>
          <span aria-hidden="true" className="text-2xl transition-transform group-open:rotate-45">+</span>
        </summary>
        <div className="border-t border-rule">
          <div className="mx-auto grid max-w-site gap-8 px-gutter py-12 lg:grid-cols-2">
            <div className="space-y-4 text-ink-2">
              <h2 className="text-title font-medium text-ink">Share chosen connections. Inspect recent calls.</h2>
              <p>TableIQ also includes a local MCP server for compatible coding agents. In Settings → MCP, select the connections to share and copy setup for Claude Code, Codex, OpenCode or Claude Desktop. Writes are off by default.</p>
              <p>Open <strong className="text-ink">MCP Activity</strong> in the top toolbar to inspect recent redacted SQL, row counts and outcomes. Full result sets are not retained in Activity, and it is not a permanent audit log.</p>
              <p>Use a least-privilege database role. Read access can expose sensitive data or run expensive queries; results an agent receives may go to its provider. Local agents need TableIQ running on the same Mac. Hosted agents cannot reach your Mac through their own localhost.</p>
              <a href={`${GITHUB}/blob/main/docs/launch/AGENT-SETUP.md`} className="inline-block underline decoration-rule underline-offset-4 hover:text-ink">Read client setup instructions</a>
            </div>
            <div><AgentTrace /></div>
          </div>
          <Proof />
          <div className="mx-auto max-w-site px-gutter pb-16"><AdvancedDemos /></div>
          <Guides />
        </div>
      </details>
    </section>
  );
}
