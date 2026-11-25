import { createContext, useContext } from "react";
import { createMoney, type Money } from "./money";

/** Onde a escolha de ocultar os valores fica guardada neste navegador. */
export const HIDE_VALUES_KEY = "prumo.hide-values";

export function readHideValues(): boolean {
  try {
    return window.localStorage.getItem(HIDE_VALUES_KEY) === "1";
  } catch {
    return false;
  }
}

export function saveHideValues(hidden: boolean): void {
  try {
    if (hidden) window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    else window.localStorage.removeItem(HIDE_VALUES_KEY);
  } catch {
    // Armazenamento bloqueado (aba privada estrita): vale até recarregar.
  }
}

export type Privacy = { money: Money; toggle: () => void };

/** Sem provedor (um componente solto em teste), os valores aparecem. */
export const PrivacyContext = createContext<Privacy>({ money: createMoney(false), toggle: () => undefined });

/** Os formatadores de valor: mascarados quando a pessoa ocultou os valores. */
export function useMoney(): Money {
  return useContext(PrivacyContext).money;
}

/** O estado do olho e o botão que o troca. */
export function usePrivacy(): { hidden: boolean; toggle: () => void } {
  const { money, toggle } = useContext(PrivacyContext);
  return { hidden: money.hidden, toggle };
}
