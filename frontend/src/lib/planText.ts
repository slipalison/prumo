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
/** A preposição do segundo nome quando os dois estão do mesmo lado: "acima do Bazin e do Graham". */
const SIDE_AND: Record<Side, string> = { above: "do", below: "do", equal: "ao" };

/**
 * O aviso do ativo sem teto: as referências públicas de Bazin e de Graham, o
 * preço de hoje e de que lado dele está cada uma, dito em palavras. É só
 * informação: o texto não recomenda nada e deixa a decisão com a pessoa.
 */
export function referenceNoteText(item: Pick<PlanItem, "unit_price">, references: PriceReferences, money: Money): string {
  const found: Array<{ name: string; value: number; side: Side }> = [];
  for (const [name, value] of [
    ["Bazin", references.bazin],
    ["Graham", references.graham],
  ] as const) {
    if (value !== null) found.push({ name, value, side: sideOf(item.unit_price, value) });
  }
  const [first, second] = found;
  let where = "";
  if (first && second) {
    where =
      first.side === second.side
        ? `${SIDE_WORD[first.side]} ${first.name} e ${SIDE_AND[first.side]} ${second.name}`
        : `${SIDE_WORD[first.side]} ${first.name}, ${SIDE_WORD[second.side]} ${second.name}`;
  } else if (first) {
    where = `${SIDE_WORD[first.side]} ${first.name}`;
  }
  const shown = joinNames(found.map(({ name, value }) => `${name} ${money.brl(value)}`));
  return `Sem teto de preço. ${found.length > 1 ? "Referências públicas" : "Referência pública"}: ${shown}; hoje ${money.brl(item.unit_price)} (${where}). A decisão de aportar é sua.`;
}
