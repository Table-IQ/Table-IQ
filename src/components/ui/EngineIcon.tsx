import type { Engine } from "../../lib/commands";

/**
 * TablePlus-style engine badge — a soft-coloured circle with a 2-letter monogram.
 * Used everywhere a connection is identified (sidebar list, ⌘O picker, MainShell
 * breadcrumb dot). Deliberately matches TablePlus's idiom rather than rendering
 * vendor logos, both for trademark hygiene and so the colour is what carries
 * recognition at a glance.
 */
export type EngineIconSize = "sm" | "md" | "lg" | "xl";

const PX: Record<EngineIconSize, number> = { sm: 16, md: 22, lg: 32, xl: 44 };
const FONT: Record<EngineIconSize, number> = { sm: 9, md: 10, lg: 12, xl: 15 };

export const ENGINE_COLORS: Record<Engine, string> = {
  postgres: "#336791",
  mysql: "#4479a1",
  sqlite: "#003b57",
  redis: "#dc382d",
  mongodb: "#13aa52",
  sqlserver: "#cc2927",
};

export const ENGINE_MONOGRAMS: Record<Engine, string> = {
  postgres: "Pg",
  mysql: "My",
  sqlite: "Sq",
  redis: "Re",
  mongodb: "Mo",
  sqlserver: "Ms",
};

export const ENGINE_LABELS: Record<Engine, string> = {
  postgres: "PostgreSQL",
  mysql: "MySQL / MariaDB",
  sqlite: "SQLite",
  redis: "Redis",
  mongodb: "MongoDB",
  sqlserver: "SQL Server",
};

export function EngineIcon({
  engine,
  size = "md",
  className = "",
  title,
}: {
  engine: Engine;
  size?: EngineIconSize;
  className?: string;
  title?: string;
}) {
  const px = PX[size];
  const font = FONT[size];
  const color = ENGINE_COLORS[engine];
  return (
    <span
      aria-hidden={!title}
      title={title ?? ENGINE_LABELS[engine]}
      className={`inline-flex shrink-0 items-center justify-center font-semibold text-white ${className}`}
      style={{
        width: px,
        height: px,
        borderRadius: "50%",
        fontSize: font,
        letterSpacing: "-0.02em",
        // TablePlus uses a flat fill plus a faint inner highlight that reads as
        // glassy depth. The inset shadow is what stops the circle from feeling
        // like a Tailwind avatar — it gives the tile a manufactured edge.
        background: color,
        boxShadow:
          "inset 0 1px 0 rgb(255 255 255 / 0.18), inset 0 -1px 0 rgb(0 0 0 / 0.18)",
      }}
    >
      {ENGINE_MONOGRAMS[engine]}
    </span>
  );
}

export function engineLabel(engine: Engine): string {
  return ENGINE_LABELS[engine] ?? engine;
}
