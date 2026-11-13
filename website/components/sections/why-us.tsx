import { StatItem } from "@/components/blocks/items";
import { SectionShell } from "@/components/blocks/section-shell";
import { Reveal } from "@/components/motion/reveal";
import { Button } from "@/components/ui/button";
import { ENGINE_TOTAL } from "@/lib/engines";

export function WhyUs() {
  return (
    <SectionShell id="why" surface="band" label="why-title">
      <Reveal className="grid gap-10 lg:grid-cols-2 lg:gap-8">
        <h2 id="why-title" className="text-section font-medium text-balance">Every supported database.<br />Free forever.</h2>
        <div className="space-y-4 text-lead text-band-muted">
          <p>Unlimited connections. Local MCP. Staged edits. SSH tunnels. Bring-your-own-key AI. No engine or device-count paywall.</p>
          <p>Credentials stay in the macOS Keychain. Connect directly to your databases. No account or required cloud service.</p>
          <Button href="/license" variant="inverse">Read the license</Button>
        </div>
      </Reveal>
      <div className="mt-16 grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
        <StatItem value={String(ENGINE_TOTAL)} label="databases and cloud services" />
        <StatItem value="$0" label="for every engine and feature" />
        <StatItem value="0" label="accounts or cloud services required" />
        <StatItem value="AGPL" label="3.0-only, open source" />
      </div>
    </SectionShell>
  );
}
