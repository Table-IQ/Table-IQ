import { ProductShot } from "@/components/blocks/product-shot";
import { Button } from "@/components/ui/button";
import { DOWNLOAD_URL, PLATFORM_STATUS, PREVIEW } from "@/lib/site";

export function Hero() {
  return (
    <section aria-labelledby="hero-title" className="overflow-hidden bg-paper">
      <div className="mx-auto grid max-w-site items-center gap-12 px-gutter pt-14 pb-16 md:pt-20 lg:grid-cols-12 lg:gap-8 lg:pb-24">
        <div className="lg:col-span-6 xl:col-span-5">
          <h1 id="hero-title" className="hero-rise text-display font-medium text-balance">
            A free, open-source database client for Mac.
          </h1>
          <p className="hero-rise-2 mt-6 max-w-xl text-lead text-ink-2">
            Open the SQL and table tabs you need, with no paid tab limit. Browse PostgreSQL, MySQL, SQLite and more, review changes, and use AI with your own API key.
          </p>
          <div className="hero-rise-2 mt-8 flex flex-wrap items-center gap-x-6 gap-y-3">
            <Button href={DOWNLOAD_URL}>Download developer preview</Button>
            <Button href="#demo" variant="text">Watch the everyday workflow</Button>
          </div>
          <p className="hero-rise-2 mt-4 max-w-xl text-sm text-ink-2">{PLATFORM_STATUS}</p>
          <p className="hero-rise-2 mt-10 flex flex-wrap gap-x-6 gap-y-1 text-sm text-muted">
            <span>No tab paywall</span>
            <span>No account required</span>
            <span>{PREVIEW.platform}</span>
          </p>
          <p className="hero-rise-2 mt-3 max-w-xl text-sm text-muted">Unsigned and not notarized by Apple. macOS will ask you to approve opening it. Start with a local database or disposable data.</p>
        </div>
        <div className="hero-rise-3 relative lg:col-span-6 xl:col-span-7">
          <figure>
            <ProductShot name="hero" priority width={2360} height={1475} alt="TableIQ interface preview showing an orders table, schema sidebar, data grid and table inspector" />
            <figcaption className="mt-3 text-sm text-muted">Interface preview with sample data. Watch the recording below to see the Mac app in use.</figcaption>
          </figure>
        </div>
      </div>
    </section>
  );
}
