import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { getDefaultTheme } from "@glideapps/glide-data-grid";
import { gridTheme, gridRowTints, nullCellStyle } from "./theme";

const css = readFileSync(new URL("../index.css", import.meta.url), "utf8");
function tokens(selector: string) {
  const block = css.slice(css.indexOf(`${selector} {`)).split("\n}")[0];
  return Object.fromEntries([...block.matchAll(/--([\w-]+):\s*(#[\da-f]{6})\s*;/gi)].map((m) => [m[1], m[2]]));
}
function luminance(hex: string) {
  const channels = [1, 3, 5].map((offset) => {
    const c = parseInt(hex.slice(offset, offset + 2), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
}
function contrast(a: string, b: string) {
  const x = luminance(a), y = luminance(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

describe("Pearl readability", () => {
  for (const mode of ["light", "dark"] as const) {
    const palette = tokens(mode === "light" ? ":root" : '[data-theme="dark"]');
    it(`${mode}: selected grid headers and markers meet WCAG AA`, () => {
      const theme = { ...getDefaultTheme(), ...gridTheme(mode) };
      expect(contrast(theme.textHeaderSelected, theme.accentColor)).toBeGreaterThanOrEqual(4.5);
      expect(contrast(theme.accentFg, theme.accentColor)).toBeGreaterThanOrEqual(4.5);
    });
    it(`${mode}: body, secondary text, and form hints meet WCAG AA on every solid surface`, () => {
      for (const fg of ["text", "muted", "muted-2", "primary-text"]) {
        for (const bg of ["bg", "surface", "surface-2", "pearl-panel"]) {
          expect(contrast(palette[fg], palette[bg]), `${fg} on ${bg}`).toBeGreaterThanOrEqual(4.5);
        }
      }
      expect(contrast("#ffffff", palette.primary), "primary button text").toBeGreaterThanOrEqual(4.5);
    });
    it(`${mode}: secondary text survives glass over black and white backdrops`, () => {
      const selector = mode === "light" ? ":root" : '[data-theme="dark"]';
      const block = css.slice(css.indexOf(`${selector} {`)).split("\n}")[0];
      const alpha = Number(block.match(/--pearl-tint:.*\/\s*([\d.]+)/)?.[1]);
      for (const backdrop of [0, 255]) {
        const blended = "#" + [1, 3, 5].map((offset) => Math.round(
          parseInt(palette["pearl-panel"].slice(offset, offset + 2), 16) * alpha + backdrop * (1 - alpha),
        ).toString(16).padStart(2, "0")).join("");
        for (const fg of ["text", "muted", "muted-2"]) expect(contrast(palette[fg], blended)).toBeGreaterThanOrEqual(4.5);
      }
    });
    it(`${mode}: SQL syntax and grid NULL values stay readable`, () => {
      for (const token of ["syn-keyword", "syn-comment", "syn-string", "syn-number", "syn-gutter"]) {
        expect(contrast(palette[token], palette.bg), token).toBeGreaterThanOrEqual(4.5);
      }
      for (const bg of [gridTheme(mode).bgCell, ...Object.values(gridRowTints(mode))]) {
        expect(contrast(nullCellStyle(mode).textDark, bg), `NULL on ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
    });
    it(`${mode}: JSON tokens stay readable on document cards`, () => {
      for (const rule of css.matchAll(/\.tiq-j-[\w-]+[^{}]*\{ color: var\(--([\w-]+)\)/g)) {
        expect(contrast(palette[rule[1]], palette.surface), rule[0]).toBeGreaterThanOrEqual(4.5);
      }
    });
  }
});
