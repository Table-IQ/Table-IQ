import { describe, it, expect } from "vitest";
import { resolveTheme, gridTheme, gridRowTints, editTints, nullCellStyle, searchHitBg } from "./theme";

describe("resolveTheme", () => {
  it("returns the explicit choice for light/dark", () => {
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("follows the system preference when auto", () => {
    expect(resolveTheme("auto", true)).toBe("dark");
    expect(resolveTheme("auto", false)).toBe("light");
  });
});

describe("gridTheme", () => {
  it("gives a dark-surfaced grid theme in dark mode", () => {
    // Updated in the Slice 22 design overhaul: dark bg shifted from `#0b0f17` to
    // the warmer-leaning `#0f1115` for macOS dark-mode comfort. See DESIGN.md.
    expect(gridTheme("dark").bgCell).toBe("#0f1115");
  });
  it("gives a light-surfaced grid theme in light mode", () => {
    expect(gridTheme("light").bgCell).toBe("#ffffff");
  });
  it("uses the brand accent in both modes", () => {
    expect(gridTheme("light").accentColor).toBe("#4f46e5");
    expect(gridTheme("dark").accentColor).toBe("#6366f1");
  });
  it("draws cells in the mono face at 12.5px, like TablePlus", () => {
    for (const mode of ["light", "dark"] as const) {
      expect(gridTheme(mode).baseFontStyle).toBe("12.5px");
      expect(gridTheme(mode).fontFamily).toMatch(/^"SF Mono"/);
    }
  });
  it("keeps the header in the sans face — its style leads with the sans stack", () => {
    // glide composes the header font as `${headerFontStyle} ${fontFamily}`.
    expect(gridTheme("light").headerFontStyle).toMatch(/^500 12\.5px -apple-system/);
  });
  it("paints light-mode data and headers pure black on a white header", () => {
    const t = gridTheme("light");
    expect([t.textDark, t.textHeader, t.bgHeader]).toEqual(["#000000", "#000000", "#ffffff"]);
  });
});

describe("gridRowTints", () => {
  it("zebra tint sits between bgCell and the chrome surface in each mode", () => {
    // Subtle alternate-row stripe (TablePlus parity). Light is just off-white;
    // dark is just above the cell black — never as strong as `--surface`.
    expect(gridRowTints("light").zebra).toBe("#f5f5f5");
    expect(gridRowTints("dark").zebra).toBe("#13151a");
  });
  it("hover tint is a neutral, distinct from the zebra stripe", () => {
    expect(gridRowTints("light").hover).toBe("#eef0f3");
    expect(gridRowTints("dark").hover).toBe("#1a1d24");
    expect(gridRowTints("light").hover).not.toBe(gridRowTints("light").zebra);
  });
});

describe("editTints", () => {
  it("distinct saving/saved/error tints per theme", () => {
    const l = editTints("light");
    expect(new Set([l.saving, l.saved, l.error]).size).toBe(3);
    const d = editTints("dark");
    expect(new Set([d.saving, d.saved, d.error]).size).toBe(3);
    expect(l.saved).not.toBe(d.saved);
  });
});

describe("nullCellStyle", () => {
  it("dims and italicises the NULL placeholder in both modes", () => {
    for (const mode of ["light", "dark"] as const) {
      const s = nullCellStyle(mode);
      expect(s.baseFontStyle).toContain("italic");
      // Same muted colour for both of glide's text slots, and never the body text.
      expect(s.textDark).toBe(s.textLight);
      expect(s.textDark).not.toBe(gridTheme(mode).textDark);
    }
  });

  it("uses the mode's own muted tone", () => {
    expect(nullCellStyle("light").textDark).toBe("#71717a");
    expect(nullCellStyle("dark").textDark).toBe("#71757e");
  });
});

describe("searchHitBg", () => {
  it("is its own colour per mode, distinct from the selected-row band", () => {
    for (const mode of ["light", "dark"] as const) {
      const hit = searchHitBg(mode);
      expect(hit).toMatch(/^#[0-9a-f]{6}$/i);
      expect(hit).not.toBe(gridTheme(mode).accentLight);
      expect(hit).not.toBe(gridTheme(mode).bgCell);
    }
    expect(searchHitBg("light")).not.toBe(searchHitBg("dark"));
  });
});
