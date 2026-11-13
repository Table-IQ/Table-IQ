import { StepItem } from "@/components/blocks/items";
import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";
import { Reveal } from "@/components/motion/reveal";
import { Button } from "@/components/ui/button";

const STEPS = [
  { title: "Share chosen connections", body: "Turn on the MCP server in Settings → MCP. It listens only on your Mac. Everything you do not select stays invisible, and writes stay off." },
  { title: "Connect your agent", body: "Claude Code and Claude Desktop have built-in configurations. Other compatible MCP clients use the local endpoint with a bearer token." },
  { title: "Inspect every call", body: "Open MCP Activity to see the SQL, returned data and refused calls. Run the same SQL yourself and compare." },
];

export function HowItWorks() {
  return (
    <SectionShell id="how" label="how-title">
      <SectionHeading id="how-title" title="Share a connection. Connect your agent. Inspect every call.">
        TableIQ’s built-in MCP server connects your coding agent to the databases you choose.
      </SectionHeading>
      <ol className="grid gap-10 md:grid-cols-3 md:gap-8">
        {STEPS.map((s, i) => (
          <li key={s.title}>
            <Reveal delay={i * 0.08}>
              <StepItem n={i + 1} title={s.title}>{s.body}</StepItem>
            </Reveal>
          </li>
        ))}
      </ol>
      <div className="mt-12 rounded-card border border-rule bg-paper-2 p-6 md:flex md:items-center md:justify-between md:gap-8">
        <p className="max-w-3xl text-ink-2">
          <strong className="text-ink">Read-only is a starting point.</strong> Use a least-privilege database role. Read access can still expose sensitive data or run expensive queries, and results your agent receives may go to its model provider.
        </p>
        <div className="mt-4 shrink-0 md:mt-0"><Button href="/guides/mcp-permissions" variant="text">How permissions work</Button></div>
      </div>
    </SectionShell>
  );
}
