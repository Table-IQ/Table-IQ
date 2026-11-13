export function BrandLogo() {
  return (
    <picture>
      <source srcSet="/img/brand/tableiq-logo-horizontal-dark.svg" data-theme-source="dark" media="all" suppressHydrationWarning />
      <img src="/img/brand/tableiq-logo-horizontal-light.svg" alt="TableIQ" width={125} height={36} />
    </picture>
  );
}
