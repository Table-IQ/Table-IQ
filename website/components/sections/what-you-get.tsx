import { ProductShot } from "@/components/blocks/product-shot";
import { SectionHeading } from "@/components/blocks/section-heading";
import { SectionShell } from "@/components/blocks/section-shell";
import { ScreenTabs } from "@/components/motion/screen-tabs";

const SCREENS = [
  { id: "query", label: "Query", caption: "Write SQL and inspect the returned rows in the same place.", shot: <ProductShot name="sql" width={1600} height={1000} alt="SQL editor with a query joining orders and customers, and its result grid below" /> },
  { id: "browse", label: "Browse", caption: "Browse your data, with the full database in view.", shot: <ProductShot name="hero" width={2360} height={1475} alt="TableIQ showing the orders table of a storefront Postgres database, with the schema sidebar, a paged data grid and the table inspector" /> },
  { id: "review", label: "Review", caption: "Inspect the exact SQL before committing staged changes.", shot: <ProductShot name="staged" width={1600} height={1000} alt="Staged cell edits in the data grid with the pending SQL ready to review" /> },
  { id: "schema", label: "Schema", caption: "Follow the foreign keys across your database.", shot: <ProductShot name="erd" width={1600} height={1000} alt="Entity relationship diagram of the storefront database: customers, orders, order items, products and shipments" /> },
];

const POINTS = [
  { title: "Write and run SQL", body: "Run multi-statement SQL, inspect result grids, save queries and see a log of every statement." },
  { title: "Review staged edits", body: "Stage edits, inspect the exact SQL, then commit in one transaction. Production connections ask first." },
  { title: "Explore your schema", body: "Inspect columns, indexes and triggers, or follow foreign keys in a schema diagram." },
];

export function WhatYouGet() {
  return (
    <SectionShell id="features" label="features-title">
      <SectionHeading id="features-title" title={<>Query data.<br />Review changes. Commit.</>}>
        Write SQL, browse results and stage edits. Review the exact SQL before applying your changes.
      </SectionHeading>
      <div className="grid gap-10 lg:grid-cols-12 lg:gap-8">
        <dl className="space-y-8 lg:col-span-4">
          {POINTS.map((p) => (
            <div key={p.title} className="border-t border-rule pt-5">
              <dt className="text-title font-medium">{p.title}</dt>
              <dd className="mt-2 text-ink-2">{p.body}</dd>
            </div>
          ))}
        </dl>
        <div className="lg:col-span-8">
          <ScreenTabs screens={SCREENS} initial="query" />
          <p className="mt-3 text-sm text-muted">Interface previews with sample data. Engine capabilities vary; see the supported database details below.</p>
        </div>
      </div>
    </SectionShell>
  );
}
