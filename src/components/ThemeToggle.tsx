import { useTheme } from "../theme/ThemeProvider";
import { IconAuto, IconMoon, IconSun } from "./ui/Icon";
import { IconButton } from "./ui/IconButton";
import type { ThemeChoice } from "../lib/theme";

/** light → auto → dark → light. Shared with the titlebar's » overflow menu entry. */
export const NEXT_THEME: Record<ThemeChoice, ThemeChoice> = { light: "auto", auto: "dark", dark: "light" };

/** Compact tri-state theme button — light / auto / dark, cycles on click. */
export function ThemeToggle() {
  const { choice, setChoice } = useTheme();
  const next = NEXT_THEME;
  const icon = choice === "light" ? <IconSun /> : choice === "dark" ? <IconMoon /> : <IconAuto />;
  const label = `Theme: ${choice} (click for ${next[choice]})`;
  return <IconButton icon={icon} label={label} onClick={() => setChoice(next[choice])} />;
}
