import type { Engine } from "@/lib/engines";

// Vendor marks sit on a Pearl tile so dark logos stay legible in both themes.
export function EngineMark({ engine, size = "sm", alt = "" }: { engine: Pick<Engine, "src" | "monogram" | "color">; size?: "sm" | "lg"; alt?: string }) {
  const box = size === "lg" ? "size-11 rounded-card" : "size-7 rounded-pill";
  const img = size === "lg" ? "size-7" : "size-5";
  if (!engine.src) {
    return <span aria-hidden="true" className={`grid shrink-0 place-items-center text-xs font-semibold text-pearl ${box}`} style={{ background: engine.color }}>{engine.monogram}</span>;
  }
  return (
    <span className={`grid shrink-0 place-items-center bg-pearl ${box}`}>
      <img src={engine.src} alt={alt} title={alt || undefined} width={size === "lg" ? 28 : 20} height={size === "lg" ? 28 : 20} loading="lazy" className={`${img} object-contain`} />
    </span>
  );
}
