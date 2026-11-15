import { Comparison } from "@/components/sections/comparison";
import { AgentIntegrations } from "@/components/sections/agent-integrations";
import { Demo } from "@/components/sections/demo";
import { JsonLd } from "@/components/blocks/json-ld";
import { Engines } from "@/components/sections/engines";
import { FaqSection } from "@/components/sections/faq";
import { FinalCta } from "@/components/sections/final-cta";
import { Hero } from "@/components/sections/hero";
import { HowItWorks } from "@/components/sections/how-it-works";
import { Problem } from "@/components/sections/problem";
import { Toolkit } from "@/components/sections/toolkit";
import { WhatYouGet } from "@/components/sections/what-you-get";
import { WhyUs } from "@/components/sections/why-us";
import { pageMeta } from "@/lib/meta";
import { APP_ID, APP_LINE, DESCRIPTION, FAQ, ORG, SITE, TITLE, graph } from "@/lib/site";

export const metadata = pageMeta("/", TITLE, DESCRIPTION);

const schema = graph(
  {
    "@type": "SoftwareApplication", "@id": APP_ID, name: "TableIQ", url: `${SITE}/`,
    applicationCategory: "DeveloperApplication", operatingSystem: "macOS", description: APP_LINE,
    offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
    license: "https://www.gnu.org/licenses/agpl-3.0.html", image: `${SITE}/img/hero-light.jpg`, publisher: { "@id": ORG["@id"] },
  },
  { "@type": "WebSite", "@id": `${SITE}/#site`, url: `${SITE}/`, name: "TableIQ", publisher: { "@id": ORG["@id"] } },
  { "@type": "FAQPage", mainEntity: FAQ.map((f) => ({ "@type": "Question", name: f.q, acceptedAnswer: { "@type": "Answer", text: f.a } })) },
);

export default function Home() {
  return (
    <>
      <JsonLd data={schema} />
      <Hero />
      <Demo />
      <WhatYouGet />
      <HowItWorks />
      <WhyUs />
      <Comparison />
      <Problem />
      <Toolkit />
      <Engines />
      <AgentIntegrations />
      <FaqSection />
      <FinalCta />
    </>
  );
}
