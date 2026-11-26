import { AnimatePresence, motion } from "motion/react";
import { THEME_LABEL, useTheme } from "../lib/theme";
import { IconMonitor, IconMoon, IconSun } from "./icons";
import { DURATION } from "../lib/motion";

const ICON = { dark: IconMoon, light: IconSun, system: IconMonitor } as const;

/** Escuro → claro → do sistema. O ícone gira ao trocar. */
export function ThemeToggle() {
  const [preference, cycle] = useTheme();
  const Icon = ICON[preference];
  return (
    <button
      type="button"
      className="theme-toggle"
      onClick={cycle}
      aria-label={`Tema ${THEME_LABEL[preference]}. Trocar tema`}
    >
      <AnimatePresence mode="wait" initial={false}>
        <motion.span
          key={preference}
          className="theme-toggle__icon"
          initial={{ rotate: -90, opacity: 0, scale: 0.6 }}
          animate={{ rotate: 0, opacity: 1, scale: 1 }}
          exit={{ rotate: 90, opacity: 0, scale: 0.6 }}
          transition={{ duration: DURATION.base }}
        >
          <Icon />
        </motion.span>
      </AnimatePresence>
      <span className="theme-toggle__label">Tema {THEME_LABEL[preference]}</span>
    </button>
  );
}
