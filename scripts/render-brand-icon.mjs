// Render the master brand tile SVG to a 1024px PNG (with alpha) for `pnpm tauri icon`.
// Usage: node scripts/render-brand-icon.mjs
import { Resvg } from "@resvg/resvg-js";
import fs from "fs";

const svg = fs.readFileSync("src/assets/brand/icon-tile.svg");
const r = new Resvg(svg, {
  fitTo: { mode: "width", value: 1024 },
  background: "rgba(0,0,0,0)",
});
fs.writeFileSync("src/assets/brand/icon-1024.png", r.render().asPng());
console.log("wrote src/assets/brand/icon-1024.png", fs.statSync("src/assets/brand/icon-1024.png").size, "bytes");
