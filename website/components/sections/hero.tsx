import { ProductShot } from "@/components/blocks/product-shot";
import { AgentTrace } from "@/components/motion/agent-trace";
import { Button } from "@/components/ui/button";

export function Hero() {
  return (
    <section aria-labelledby="hero-title" className="overflow-hidden bg-paper">
      <div className="mx-auto grid max-w-site items-center gap-12 px-gutter pt-14 pb-16 md:pt-20 lg:grid-cols-12 lg:gap-8 lg:pb-24">
        <div className="lg:col-span-6 xl:col-span-5">
          <h1 id="hero-title" className="hero-rise text-display font-medium text-balance">
            Your Mac database client. See every agent query.
          </h1>
          <p className="hero-rise-2 mt-6 max-w-xl text-lead text-ink-2">
            Browse, query and edit PostgreSQL, MySQL, SQLite and more. Let Claude Code or Cursor query chosen connections, read-only by default, and inspect their SQL and results.
          </p>
          <div className="hero-rise-2 mt-8 flex flex-wrap items-center gap-x-6 gap-y-3">
            <Button href="/guides/postgres-investigation">Read the walkthrough</Button>
            <Button href="#how" variant="text">See how agents connect</Button>
          </div>
          <p className="hero-rise-2 mt-10 flex flex-wrap gap-x-6 gap-y-1 text-sm text-muted">
            <span>Free and open source</span>
            <span>Built in Rust for macOS</span>
            <span>Download coming soon</span>
          </p>
        </div>
        <div className="hero-rise-3 relative lg:col-span-6 xl:col-span-7">
          <ProductShot name="hero" priority width={2360} height={1475} alt="TableIQ showing the orders table of a storefront Postgres database, with the schema sidebar, a paged data grid and the table inspector" />
          <div className="relative mt-4 md:absolute md:-bottom-12 md:-left-10 md:mt-0 md:w-5/6">
            <AgentTrace />
          </div>
        </div>
      </div>
    </section>
  );
}
