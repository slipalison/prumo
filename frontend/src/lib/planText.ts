import type { Asset, PlanItem, PlanNotice, PriceReferences } from "../api/types";
import { acceptsB3Listing, CLASS_META, looksLikeB3Ticker, nounOf, unitOf } from "./assetClasses";
import { formatPercent, plural } from "./format";
import type { Money } from "./money";
import { findAssetByLabel } from "./rating";

/**
 * Ativo sem cotação que parece um ETF ou BDR da B3 cadastrado sem a marca —
 * o caso de um ETF de bitcoin em Cripto procurado como se fosse cripto.
 */
export function looksMissingB3Flag(asset: Pick<Asset, "asset_class" | "symbol" | "listed_on_b3">): boolean {
  return acceptsB3Listing(asset.asset_class) && !asset.listed_on_b3 && looksLikeB3Ticker(asset.symbol ?? "");
}

function blockedText(notice: Extract<PlanNotice, { code: "class_blocked" }>, money: Money): string {
  const lead = `${CLASS_META[notice.asset_class].label} precisa de ${money.compact(notice.amount)} para chegar na meta`;
  switch (notice.reason) {
    case "no_assets":
      return `${lead} e ainda não tem nenhum ativo.`;
    case "unrated":
      return `${lead}, mas nenhum ativo dela tem nota. Dê nota a eles e o aporte chega lá.`;
    case "unpriced":
      return `${lead}, mas os ativos com nota estão sem cotação.`;
    case "above_ceiling":
      return `${lead}, mas os ativos com nota estão acima do teto de preço que você definiu. Espere o preço voltar ou revise o teto.`;
    case "concentration_limit":
      return `${lead}, mas os ativos dela já estão no seu limite por ativo. Cadastre outro ativo na classe ou suba o limite.`;
  }
}

/** "A", "A e B", "A, B e C". */
function joinNames(names: string[]): string {
  if (names.length <= 1) return names.join("");
  return `${names.slice(0, -1).join(", ")} e ${names[names.length - 1]}`;
}

function limitText(notice: Extract<PlanNotice, { code: "concentration_limit" }>, money: Money): string {
  const limit = formatPercent(notice.max_percent, 0);
  const left = notice.amount >= 0.01;
  if (notice.symbols.length === 0) {
    return `${money.brl(notice.amount)} ficaram sem destino: nenhum ativo cabe mais sem passar do seu limite de ${limit} da carteira.`;
  }
  const one = notice.symbols.length === 1;
  const lead = `${joinNames(notice.symbols)} ${one ? "chegou" : "chegaram"} no seu limite de ${limit} da carteira`;
  return left
    ? `${lead}. ${money.brl(notice.amount)} ficaram sem destino: não cabem em nenhum ativo sem passar do limite.`
    : `${lead}; o que passaria foi para os outros ativos da classe.`;
}

/** O aviso do plano em uma frase que diz o que fazer. */
export function noticeText(notice: PlanNotice, assets: Asset[], money: Money): string {
  switch (notice.code) {
    case "class_blocked":
      return blockedText(notice, money);
    case "assets_without_price": {
      const one = notice.symbols.length === 1;
      const b3Hint = notice.symbols
        .map((label) => findAssetByLabel(assets, label))
        .some((asset) => asset !== undefined && looksMissingB3Flag(asset));
      return `Sem cotação agora: ${notice.symbols.join(", ")}. ${one ? "Fica" : "Ficam"} fora da conta até ter preço.${
        b3Hint ? " Se for ETF ou BDR da B3, ligue “Negociado na B3” no ativo." : ""
      }`;
    }
    case "assets_without_score":
      return `Sem nota: ${notice.symbols.join(", ")}. Responda o checklist ${notice.symbols.length === 1 ? "dele" : "deles"} para receber aporte.`;
    case "no_targets":
      return "Você ainda não definiu metas. Sem elas eu não sei para onde levar a carteira.";
    case "asset_above_ceiling":
      return `${notice.symbol} está a ${money.brl(notice.unit_price)}, acima do seu teto de ${money.brl(notice.max_price)}. Fica fora deste aporte.`;
    case "concentration_limit":
      return limitText(notice, money);
  }
}

/** "17 cotas × R$ 53,81", "0,0045 BTC × R$ …", "0,39 título × R$ …" ou "Aplicar este valor". */
export function unitLine(item: PlanItem, money: Money): string {
  const unit = unitOf(item.asset_class, item.listed_on_b3, item.treasury);
  if (unit === "value") return "Aplicar este valor";
  if (unit === "fraction" && !item.treasury) {
    return `${money.quantity(item.quantity, true)} ${item.symbol ?? ""} × ${money.brl(item.unit_price)}`;
  }
  return `${money.quantity(item.quantity, unit === "fraction")} ${plural(item.quantity, nounOf(item.asset_class, item.listed_on_b3, item.treasury))} × ${money.brl(item.unit_price)}`;
}

type Side = "above" | "below" | "equal";

/** De que lado da referência o preço de hoje está, no centavo. */
function sideOf(price: number, reference: number): Side {
  const cents = Math.round(price * 100);
  const referenceCents = Math.round(reference * 100);
  if (cents === referenceCents) return "equal";
  return cents > referenceCents ? "above" : "below";
}

const SIDE_WORD: Record<Side, string> = { above: "acima do", below: "abaixo do", equal: "igual ao" };

/** O que cada método é, em uma frase, para a explicação do chip. */
const METHOD_TIP = {
  Bazin: "Bazin: o preço em que a média de dividendos por cota renderia o seu rendimento-alvo.",
  Graham: "Graham: a raiz de 22,5 × lucro por ação × patrimônio por ação, dos balanços da CVM.",
} as const;

/** Um método de referência no cartão do ativo sem teto. */
export type ReferenceCheck = {
  name: keyof typeof METHOD_TIP;
  value: number;
  /** Preço de hoje até o do método (no centavo; igual conta, como no teto). */
  approves: boolean;
  /** O nome acessível do chip, já com o veredito: a cor e o ícone não falam sozinhos. */
  label: string;
  /** A explicação que abre no toque. */
  tip: string;
};

/**
 * As referências públicas do ativo sem teto, uma por método: o preço de cada
 * uma e se o preço de hoje passa nela ("o método aprova a compra a esse
 * preço"). É só informação: o plano não muda e a decisão é da pessoa.
 */
export function referenceChecks(
  item: Pick<PlanItem, "unit_price">,
  references: PriceReferences,
  money: Money,
): ReferenceCheck[] {
  const checks: ReferenceCheck[] = [];
  for (const [name, value] of [
    ["Bazin", references.bazin],
    ["Graham", references.graham],
  ] as const) {
    if (value === null) continue;
    const side = sideOf(item.unit_price, value);
    const approves = side !== "above";
    const verdict = approves ? "aprova" : "não aprova";
    checks.push({
      name,
      value,
      approves,
      label: `${name} ${money.brl(value)}: ${verdict}`,
      tip: `${METHOD_TIP[name]} Hoje ${money.brl(item.unit_price)}, ${SIDE_WORD[side]} preço do método (${money.brl(value)}): ${verdict} a compra a esse preço.`,
    });
  }
  return checks;
}

/** A explicação do "Sem teto": o que são as referências e de quem é a decisão. */
export const NO_CEILING_TIP =
  "Você não definiu teto de preço para este ativo, e o aporte segue sem ele. Bazin e Graham são referências públicas, não recomendação: a decisão de aportar é sua. Para usar um teto, edite o ativo.";
