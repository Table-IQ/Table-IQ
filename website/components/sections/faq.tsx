import Link from "next/link";
import { SectionShell } from "@/components/blocks/section-shell";
import { AccordionItem } from "@/components/ui/accordion";
import { FAQ, type Faq } from "@/lib/site";

function Answer({ item }: { item: Faq }) {
  if (!item.link) return <p>{item.a}</p>;
  const [before, after] = item.a.split(item.link.text);
  return <p>{before}<Link href={item.link.href} className="text-ink underline decoration-rule underline-offset-4">{item.link.text}</Link>{after}</p>;
}

export function FaqSection() {
  return (
    <SectionShell id="faq" label="faq-title">
      <div className="relative grid gap-10 lg:grid-cols-12 lg:gap-8">
        {/* Static dot field: texture only, no motion (motion budget is spent on the hero). */}
        <div aria-hidden="true" className="dot-field pointer-events-none absolute inset-y-0 left-0 hidden w-1/3 lg:block" />
        <h2 id="faq-title" className="relative text-section font-medium lg:col-span-4">Before you connect.</h2>
        <div className="faq relative lg:col-span-8">
          {FAQ.map((item) => (
            <AccordionItem key={item.q} title={item.q}><Answer item={item} /></AccordionItem>
          ))}
        </div>
      </div>
    </SectionShell>
  );
}
