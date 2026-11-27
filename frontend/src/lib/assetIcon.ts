import type { AssetClass } from "../api/types";
import { guessClassFromSymbol, isTreasuryKey } from "./assetClasses";

/** O que o ícone desenha quando não há logo. */
export type IconKind = "treasury" | "bitcoin" | "crypto" | "initials";

export type IconSubject = {
  symbol?: string | null;
  assetClass?: AssetClass | null;
  treasury?: boolean;
};

/**
 * A classe do ativo para a cor: a que a tela sabe, ou o palpite pelo ticker
 * (`KNRI11` é FII, `BTC` é cripto) onde a lista não traz a classe (proventos,
 * IR). `null` = sem como dizer, e o ícone fica neutro.
 */
export function iconClass({ symbol, assetClass, treasury }: IconSubject): AssetClass | null {
  if (assetClass) return assetClass;
  if (treasury || isTreasuryKey(symbol)) return "fixed_income";
  return symbol ? guessClassFromSymbol(symbol) : null;
}

/**
 * Tesouro e cripto têm ícone próprio desenhado aqui (sem fonte externa); o
 * resto cai nas iniciais. O logo de verdade, quando o servidor tem, vence tudo
 * isso — inclusive num ETF de bitcoin da B3.
 */
export function iconKind(subject: IconSubject): IconKind {
  if (subject.treasury || isTreasuryKey(subject.symbol)) return "treasury";
  if (iconClass(subject) === "crypto") {
    return subject.symbol?.toUpperCase() === "BTC" ? "bitcoin" : "crypto";
  }
  return "initials";
}

/**
 * Duas letras para o círculo: as do ticker (`ITSA4` → IT) ou, sem ticker
 * (renda fixa por valor), as iniciais do nome (`Caixa Reserva` → CR).
 */
export function initialsOf(symbol?: string | null, name?: string | null): string {
  const ticker = symbol && !isTreasuryKey(symbol) ? symbol.toUpperCase().replaceAll(/[^A-Z0-9]/g, "") : "";
  if (ticker) return ticker.slice(0, 2);
  const words = (name ?? "")
    .normalize("NFD")
    .replaceAll(/[^A-Za-z0-9\s]/g, "")
    .toUpperCase()
    .split(/\s+/)
    .filter(Boolean);
  const [first, second] = words;
  if (!first) return "•";
  return second ? `${first.charAt(0)}${second.charAt(0)}` : first.slice(0, 2);
}

/**
 * Ícone de um rótulo que pode ser um ticker (`ITSA4`) ou um nome (`Tesouro
 * Selic 2029`, `Caixa`): é o que as listas de aviso e de pagadores trazem.
 */
export function iconSubjectOfLabel(label: string): { symbol: string | null; name: string } {
  const ticker = /^[A-Za-z0-9.]{1,12}$/.test(label.trim());
  return { symbol: ticker ? label.trim() : null, name: label };
}
