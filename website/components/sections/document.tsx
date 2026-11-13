import { readFileSync } from "node:fs";
import path from "node:path";

// Guides and licensing keep their supplied security and legal wording verbatim (decisions D-05).
// The fragment is our own repository content, read at build time, never user input.
export function DocumentSection({ fragment }: { fragment: string }) {
  const html = readFileSync(path.join(process.cwd(), "content", `${fragment}.html`), "utf8");
  return (
    <div className="mx-auto max-w-site px-gutter py-16 md:py-24">
      <article className="document" dangerouslySetInnerHTML={{ __html: html }} />
    </div>
  );
}
