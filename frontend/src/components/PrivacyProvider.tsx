import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { createMoney } from "../lib/money";
import { PrivacyContext, readHideValues, saveHideValues } from "../lib/privacy";

/** Guarda se os valores estão ocultos e entrega os formatadores a quem pedir. */
export function PrivacyProvider({ children }: Readonly<{ children: ReactNode }>) {
  const [hidden, setHidden] = useState(readHideValues);

  const toggle = useCallback(() => setHidden((current) => !current), []);
  useEffect(() => saveHideValues(hidden), [hidden]);

  const value = useMemo(() => ({ money: createMoney(hidden), toggle }), [hidden, toggle]);
  return <PrivacyContext.Provider value={value}>{children}</PrivacyContext.Provider>;
}
