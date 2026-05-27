import { useTheme } from "../theme/ThemeProvider";
import { IconAuto, IconMoon, IconSun } from "./ui/Icon";
import { IconButton } from "./ui/IconButton";
import type { ThemeChoice } from "../lib/theme";

/** Compact tri-state theme button — light / auto / dark, cycles on click. */
export function ThemeToggle() {
  const { choice, setChoice } = useTheme();
  const next: Record<ThemeChoice, ThemeChoice> = { light: "auto", auto: "dark", dark: "light" };
  const icon = choice === "light" ? <IconSun /> : choice === "dark" ? <IconMoon /> : <IconAuto />;
  const label = `Theme: ${choice} (click for ${next[choice]})`;
  return <IconButton icon={icon} label={label} onClick={() => setChoice(next[choice])} />;
}
