import { useCallback, useState } from "react";

export type ThemePreference = "dark" | "light" | "system";

export const THEME_KEY = "prumo.theme";

export function readThemePreference(): ThemePreference {
  try {
    const saved = window.localStorage.getItem(THEME_KEY);
    return saved === "light" || saved === "system" ? saved : "dark";
  } catch {
    return "dark";
  }
}

/** "system" REMOVE o atributo: a media query manda e acompanha o sistema ao vivo. */
export function applyTheme(preference: ThemePreference): void {
  const root = document.documentElement;
  if (preference === "system") delete root.dataset.theme;
  else root.dataset.theme = preference;
}

export function saveThemePreference(preference: ThemePreference): void {
  try {
    window.localStorage.setItem(THEME_KEY, preference);
  } catch {
    // Armazenamento bloqueado (aba privada estrita): o tema vale até recarregar.
  }
  applyTheme(preference);
}

const NEXT: Record<ThemePreference, ThemePreference> = { dark: "light", light: "system", system: "dark" };

export const THEME_LABEL: Record<ThemePreference, string> = {
  dark: "escuro",
  light: "claro",
  system: "do sistema",
};

export function useTheme(): [ThemePreference, () => void] {
  const [preference, setPreference] = useState<ThemePreference>(readThemePreference);
  const cycle = useCallback(() => {
    const next = NEXT[preference];
    saveThemePreference(next);
    setPreference(next);
  }, [preference]);
  return [preference, cycle];
}
