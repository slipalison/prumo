import type { LivePrices, Portfolio } from "../api/types";

/**
 * Leva os números de um evento `prices` para a carteira que a tela já tem.
 *
 * Meta e contagem de ativos ficam como estão: elas vêm do `GET /portfolio`,
 * e uma meta recém-salva (otimista) não pisca de volta com um evento que saiu
 * antes do salvamento. Ativo que o evento não traz também fica — ele chega no
 * próximo `GET`.
 */
export function mergePrices(portfolio: Portfolio, prices: LivePrices): Portfolio {
  const assets = new Map(prices.assets.map((asset) => [asset.id, asset]));
  const classes = new Map(prices.classes.map((summary) => [summary.asset_class, summary]));
  return {
    ...portfolio,
    total_value: prices.total_value,
    quotes_updated_at: prices.quotes_updated_at,
    profit: prices.profit,
    classes: portfolio.classes.map((summary) => {
      const live = classes.get(summary.asset_class);
      return live
        ? {
            ...summary,
            current_value: live.current_value,
            current_percent: live.current_percent,
            eligible: live.eligible,
          }
        : summary;
    }),
    assets: portfolio.assets.map((asset) => {
      const live = assets.get(asset.id);
      return live ? { ...asset, ...live } : asset;
    }),
  };
}
