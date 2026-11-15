// Interface previews. Dark/light variants follow the site theme (theme script flips the source).
// Each capture ships at three widths; static export has no image optimizer. Variants: 800w, 1200w, original.
export function ProductShot({ name, alt, width, height, priority = false, sizes = "(min-width: 1024px) 60vw, 100vw", className = "" }: { name: string; alt: string; width: number; height: number; priority?: boolean; sizes?: string; className?: string }) {
  const set = (theme: string) => `/img/${name}-${theme}-800.jpg 800w, /img/${name}-${theme}-1200.jpg 1200w, /img/${name}-${theme}.jpg ${width}w`;
  return (
    <picture>
      <source srcSet={set("dark")} sizes={sizes} data-theme-source="dark" media="all" suppressHydrationWarning />
      <img
        src={`/img/${name}-light.jpg`}
        srcSet={set("light")}
        sizes={sizes}
        alt={alt}
        width={width}
        height={height}
        loading={priority ? "eager" : "lazy"}
        fetchPriority={priority ? "high" : undefined}
        decoding="async"
        className={`block h-auto w-full rounded-card border border-rule ${className}`}
      />
    </picture>
  );
}
