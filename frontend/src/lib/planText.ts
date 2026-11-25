import type { Asset, PlanItem, PlanNotice } from "../api/types";
import { acceptsB3Listing, CLASS_META, looksLikeB3Ticker, nounOf, unitOf } from "./assetClasses";
import { plural } from "./format";
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
  }
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
  }
}

/** "17 cotas × R$ 53,81", "0,0045 BTC × R$ …" ou "Aplicar este valor". */
export function unitLine(item: PlanItem, money: Money): string {
  const unit = unitOf(item.asset_class, item.listed_on_b3);
  if (unit === "value") return "Aplicar este valor";
  if (unit === "fraction") {
    return `${money.quantity(item.quantity, true)} ${item.symbol ?? ""} × ${money.brl(item.unit_price)}`;
  }
  return `${money.quantity(item.quantity, false)} ${plural(item.quantity, nounOf(item.asset_class, item.listed_on_b3))} × ${money.brl(item.unit_price)}`;
}
