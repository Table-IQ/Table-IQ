/**
 * Hand-rolled icon set, no external dep. 14-px viewBox, currentColor, inherits
 * text size from parent. Stroke 1.7 is the readability sweet spot — light
 * enough to feel like a system icon, heavy enough to be unambiguous at 14-16
 * px (lucide-react ships at 2; we go a hair under since our chrome is denser).
 * Keep names stable — consumed across every surface.
 */
import type { SVGProps } from "react";

type P = SVGProps<SVGSVGElement> & { size?: number };
const base = ({ size = 14, ...rest }: P) => ({
  width: size,
  height: size,
  viewBox: "0 0 14 14",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.7,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  ...rest,
});

export const IconTable = (p: P) => (
  <svg {...base(p)}>
    <rect x="1.5" y="2.5" width="11" height="9" rx="1" />
    <path d="M1.5 5.5h11M5 5.5v6" />
  </svg>
);
export const IconView = (p: P) => (
  <svg {...base(p)}>
    <path d="M1 7s2-3.5 6-3.5S13 7 13 7s-2 3.5-6 3.5S1 7 1 7Z" />
    <circle cx="7" cy="7" r="1.6" fill="currentColor" stroke="none" />
  </svg>
);
export const IconFunction = (p: P) => (
  <svg {...base(p)}>
    <path d="M9.5 2.5h-1a1.5 1.5 0 0 0-1.5 1.5v6a1.5 1.5 0 0 1-1.5 1.5h-1M4 7h5" />
  </svg>
);
export const IconSearch = (p: P) => (
  <svg {...base(p)}>
    <circle cx="6" cy="6" r="3.8" />
    <path d="m9 9 3 3" />
  </svg>
);
export const IconPlus = (p: P) => (
  <svg {...base(p)}>
    <path d="M7 3v8M3 7h8" />
  </svg>
);
export const IconPlay = (p: P) => (
  <svg {...base(p)}>
    <path d="M4 3.2v7.6L11 7 4 3.2Z" fill="currentColor" />
  </svg>
);
export const IconStop = (p: P) => (
  <svg {...base(p)}>
    <rect x="3.5" y="3.5" width="7" height="7" rx="0.8" fill="currentColor" />
  </svg>
);
export const IconRefresh = (p: P) => (
  // Single 3/4-circle arrow + sharp arrowhead at the top — reads instantly as
  // "reload" the way TablePlus / VS Code / Finder reload buttons do. The
  // previous two-arc design was too abstract to register at 12-14 px.
  <svg {...base(p)}>
    <path d="M11.5 7a4.5 4.5 0 1 1-1.4-3.25" />
    <path d="M11.6 1.5v3h-3" />
  </svg>
);
export const IconCommit = (p: P) => (
  <svg {...base(p)}>
    <path d="m3 7 3 3 5-6" />
  </svg>
);
export const IconDiscard = (p: P) => (
  <svg {...base(p)}>
    <path d="M3.5 3.5l7 7M10.5 3.5l-7 7" />
  </svg>
);
export const IconClose = (p: P) => (
  <svg {...base(p)}>
    <path d="M3.5 3.5l7 7M10.5 3.5l-7 7" />
  </svg>
);
export const IconChevron = (p: P) => (
  <svg {...base(p)}>
    <path d="m4.5 3 4 4-4 4" />
  </svg>
);
export const IconChevronDown = (p: P) => (
  <svg {...base(p)}>
    <path d="m3 4.5 4 4 4-4" />
  </svg>
);
export const IconChevronLeft = (p: P) => (
  <svg {...base(p)}>
    <path d="m9.5 3-4 4 4 4" />
  </svg>
);
export const IconChevronRight = (p: P) => (
  <svg {...base(p)}>
    <path d="m4.5 3 4 4-4 4" />
  </svg>
);
export const IconCommand = (p: P) => (
  <svg {...base(p)}>
    <path d="M5 3.5a1.5 1.5 0 1 0 1.5 1.5v4A1.5 1.5 0 1 0 8 7.5h-2A1.5 1.5 0 1 0 7.5 9v-4A1.5 1.5 0 1 0 9 6.5h-2" />
  </svg>
);
export const IconErd = (p: P) => (
  <svg {...base(p)}>
    <rect x="1.5" y="1.5" width="4" height="3" rx="0.5" />
    <rect x="8.5" y="9.5" width="4" height="3" rx="0.5" />
    <path d="M5.5 3.5h2.5a1 1 0 0 1 1 1v5" />
  </svg>
);
export const IconStructure = (p: P) => (
  <svg {...base(p)}>
    <rect x="1.5" y="2.5" width="11" height="9" rx="1" />
    <path d="M1.5 5.5h11M1.5 8.5h11M5 5.5v6" />
  </svg>
);
export const IconDiff = (p: P) => (
  <svg {...base(p)}>
    <path d="M4.5 1.5v8M4.5 9.5l-2-2M4.5 9.5l2-2M9.5 12.5v-8M9.5 4.5l-2 2M9.5 4.5l2 2" />
  </svg>
);
export const IconImport = (p: P) => (
  <svg {...base(p)}>
    <path d="M7 1.5v7M7 8.5 4.5 6M7 8.5 9.5 6" />
    <path d="M2.5 9.5v2A1 1 0 0 0 3.5 12.5h7A1 1 0 0 0 11.5 11.5v-2" />
  </svg>
);
export const IconExport = (p: P) => (
  <svg {...base(p)}>
    <path d="M7 8.5v-7M7 1.5 4.5 4M7 1.5 9.5 4" />
    <path d="M2.5 9.5v2A1 1 0 0 0 3.5 12.5h7A1 1 0 0 0 11.5 11.5v-2" />
  </svg>
);
export const IconPlugins = (p: P) => (
  <svg {...base(p)}>
    <path d="M5 1.5v2M9 1.5v2M3 5.5a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v3a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3v-3Z" />
  </svg>
);
export const IconAi = (p: P) => (
  <svg {...base(p)}>
    <path d="M7 2v3M7 9v3M2 7h3M9 7h3M3.5 3.5l2 2M8.5 8.5l2 2M3.5 10.5l2-2M8.5 5.5l2-2" />
  </svg>
);
export const IconSidebar = (p: P) => (
  <svg {...base(p)}>
    <rect x="1.5" y="2.5" width="11" height="9" rx="1" />
    <path d="M5.5 2.5v9" />
  </svg>
);
export const IconDatabase = (p: P) => (
  // Storage cylinder — the universal database glyph (TablePlus, DataGrip, every
  // ER diagram editor). Top ellipse + side walls + a faint middle band so the
  // "stack of platters" reading lands at 12-14 px.
  <svg {...base(p)}>
    <ellipse cx="7" cy="3" rx="4.5" ry="1.6" />
    <path d="M2.5 3v8c0 .9 2 1.6 4.5 1.6S11.5 11.9 11.5 11V3" />
    <path d="M2.5 7c0 .9 2 1.6 4.5 1.6S11.5 7.9 11.5 7" />
  </svg>
);
export const IconSidebarRight = (p: P) => (
  <svg {...base(p)}>
    <rect x="1.5" y="2.5" width="11" height="9" rx="1" />
    <path d="M8.5 2.5v9" />
  </svg>
);
export const IconSun = (p: P) => (
  <svg {...base(p)}>
    <circle cx="7" cy="7" r="2.5" />
    <path d="M7 1.5v1.5M7 11v1.5M1.5 7h1.5M11 7h1.5M3 3l1 1M10 10l1 1M3 11l1-1M10 4l1-1" />
  </svg>
);
export const IconMoon = (p: P) => (
  <svg {...base(p)}>
    <path d="M11 8.5a4 4 0 1 1-4.5-5 3.5 3.5 0 0 0 4.5 5Z" />
  </svg>
);
export const IconAuto = (p: P) => (
  <svg {...base(p)}>
    <circle cx="7" cy="7" r="5" />
    <path d="M7 2v10a5 5 0 0 0 0-10Z" fill="currentColor" />
  </svg>
);
export const IconLink = (p: P) => (
  <svg {...base(p)}>
    <path d="M5.5 8.5a2 2 0 0 0 3 0L11 6a2.5 2.5 0 0 0-3.5-3.5L7 3M8.5 5.5a2 2 0 0 0-3 0L3 8a2.5 2.5 0 0 0 3.5 3.5L7 11" />
  </svg>
);
export const IconChip = (p: P) => (
  <svg {...base(p)}>
    <rect x="3" y="3" width="8" height="8" rx="1" />
    <path d="M5.5 1.5v1.5M8.5 1.5v1.5M5.5 11v1.5M8.5 11v1.5M1.5 5.5h1.5M1.5 8.5h1.5M11 5.5h1.5M11 8.5h1.5" />
  </svg>
);
export const IconHelp = (p: P) => (
  <svg {...base(p)}>
    <circle cx="7" cy="7" r="5.5" />
    <path d="M5.5 5.5a1.5 1.5 0 1 1 2.3 1.3c-.5.3-.8.7-.8 1.2v.5" />
    <path d="M7 10.5v.2" />
  </svg>
);
export const IconKey = (p: P) => (
  <svg {...base(p)}>
    <circle cx="4" cy="7" r="2" />
    <path d="M6 7h6.5M11 7v2M12.5 7v1.5" />
  </svg>
);
export const IconRow = (p: P) => (
  <svg {...base(p)}>
    <rect x="1.5" y="5.5" width="11" height="3" rx="0.6" />
  </svg>
);
export const IconFilter = (p: P) => (
  <svg {...base(p)}>
    <path d="M1.5 3h11l-4 4.5v4l-3-1.5v-2.5L1.5 3Z" />
  </svg>
);
export const IconDrag = (p: P) => (
  <svg {...base(p)}>
    <circle cx="5" cy="3.5" r="0.8" fill="currentColor" stroke="none" />
    <circle cx="9" cy="3.5" r="0.8" fill="currentColor" stroke="none" />
    <circle cx="5" cy="7" r="0.8" fill="currentColor" stroke="none" />
    <circle cx="9" cy="7" r="0.8" fill="currentColor" stroke="none" />
    <circle cx="5" cy="10.5" r="0.8" fill="currentColor" stroke="none" />
    <circle cx="9" cy="10.5" r="0.8" fill="currentColor" stroke="none" />
  </svg>
);
export const IconPin = (p: P) => (
  <svg {...base(p)}>
    <path d="M7 1.5v4M5 5.5h4l1.5 3h-7L5 5.5ZM7 8.5v4" />
  </svg>
);
