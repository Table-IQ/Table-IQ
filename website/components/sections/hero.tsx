import { ProductShot } from "@/components/blocks/product-shot";
import { Button } from "@/components/ui/button";
import { DOWNLOAD_URL, PREVIEW } from "@/lib/site";

export function Hero() {
  return (
    <section aria-labelledby="hero-title" className="overflow-hidden bg-paper">
      <div className="mx-auto max-w-site px-gutter pt-14 pb-16 md:pt-24 md:pb-24">
        <div className="mx-auto max-w-4xl text-center">
          <p className="hero-rise mb-5 text-sm text-muted">TableIQ for Mac</p>
          <h1 id="hero-title" className="hero-rise text-display font-medium text-balance">
            <span className="block">A database client.</span>
            <span className="block">Free and open source.</span>
          </h1>
          <p className="hero-rise-2 mx-auto mt-6 max-w-2xl text-lead text-ink-2">
            Browse, query and review your data without a tab paywall. Optional AI uses your own key.
          </p>
          <div className="hero-rise-2 mt-8 flex flex-wrap items-center justify-center gap-x-6 gap-y-3">
            <Button href={DOWNLOAD_URL}>Download for Mac</Button>
            <Button href="#demo" variant="text">Watch demo</Button>
          </div>
          <p className="hero-rise-2 mx-auto mt-5 max-w-2xl text-xs leading-relaxed text-muted">{PREVIEW.platform} · Unsigned / not notarized · Windows and Linux coming soon.</p>
        </div>
        <div className="hero-rise-3 mt-12 md:mt-16">
          <figure>
            <ProductShot name="hero" priority sizes="(min-width: 1440px) 1312px, 92vw" width={2360} height={1475} alt="TableIQ interface preview showing an orders table, schema sidebar, data grid and table inspector" />
            <figcaption className="mt-3 text-center text-xs text-muted">Interface preview with sample data. No account required.</figcaption>
          </figure>
        </div>
      </div>
    </section>
  );
}
