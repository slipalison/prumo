import type { PlanItem, PlanNotice } from "../api/types";
import { CLASS_META } from "./assetClasses";
import { formatBRL, formatCompactBRL, formatQuantity, plural } from "./format";

/** O aviso do plano em uma frase que diz o que fazer. */
export function noticeText(notice: PlanNotice): string {
  switch (notice.code) {
    case "class_blocked":
      return `${CLASS_META[notice.asset_class].label} precisa de ${formatCompactBRL(notice.amount)} para chegar na meta, mas não tem nenhum ativo com nota. Adicione um ativo lá na Carteira.`;
    case "assets_without_price":
      return `Sem cotação agora: ${notice.symbols.join(", ")}. Eles ficam fora da conta até ter preço.`;
    case "assets_without_score":
      return `Sem nota: ${notice.symbols.join(", ")}. Responda o checklist deles para receberem aporte.`;
    case "no_targets":
      return "Você ainda não definiu metas. Sem elas eu não sei para onde levar a carteira.";
  }
}

/** "17 cotas × R$ 53,81", "0,0045 BTC × R$ …" ou "Aplicar este valor". */
export function unitLine(item: PlanItem): string {
  const meta = CLASS_META[item.asset_class];
  if (meta.unit === "value") return "Aplicar este valor";
  if (meta.unit === "fraction") {
    return `${formatQuantity(item.quantity, true)} ${item.symbol ?? ""} × ${formatBRL(item.unit_price)}`;
  }
  return `${formatQuantity(item.quantity, false)} ${plural(item.quantity, meta.noun)} × ${formatBRL(item.unit_price)}`;
}
