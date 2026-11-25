import type { Asset } from "../api/types";

/**
 * Até quantos pontos percentuais abaixo da fatia ideal ainda conta como "no
 * ideal". Sem folga, qualquer centavo de diferença acenderia o selo, e ele
 * piscaria a cada preço novo do stream.
 */
export const IDEAL_TOLERANCE = 0.5;

export type IdealReading = {
  /** O ativo entra na divisão do aporte (nota > 0 e preço): só então tem ideal. */
  counts: boolean;
  /** Quanto ele vale entre os ativos da classe que entram na divisão, de 0 a 100. */
  currentShare: number;
  /** A fatia que o motor mira, de 0 a 100 (`ideal_class_percent`). */
  idealShare: number;
  /** Abaixo da fatia ideal por mais que a folga. */
  below: boolean;
};

/**
 * O que a Carteira mostra do ideal de um ativo: a % atual, a % ideal e se está
 * abaixo. A % ideal vem do servidor — é a fatia que o motor usa; aqui só se
 * mede o ativo contra ela, na mesma base: o valor dos ativos que entram na
 * divisão. Ativo sem nota ou sem preço (ideal 0) ocupa espaço na classe, mas
 * fica fora dessa base — como no motor. É puro e vale para o valor ao vivo: o
 * evento `prices` leva `current_value` e `ideal_class_percent`.
 */
export function readIdeal(asset: Asset, classAssets: Asset[]): IdealReading {
  const counts = asset.ideal_class_percent > 0;
  if (!counts) return { counts, currentShare: 0, idealShare: 0, below: false };
  const base = classAssets.filter((a) => a.ideal_class_percent > 0).reduce((sum, a) => sum + a.current_value, 0);
  const currentShare = base > 0 ? (asset.current_value / base) * 100 : 0;
  const idealShare = asset.ideal_class_percent;
  return { counts, currentShare, idealShare, below: idealShare - currentShare > IDEAL_TOLERANCE };
}

/** Quantos ativos da carteira estão abaixo da fatia ideal da própria classe. */
export function countBelowIdeal(assets: Asset[]): number {
  return assets.filter(
    (asset) =>
      readIdeal(
        asset,
        assets.filter((other) => other.asset_class === asset.asset_class),
      ).below,
  ).length;
}

/** Algum ativo da carteira tem ideal — senão "N abaixo do ideal" não faz sentido. */
export function hasIdeal(assets: Asset[]): boolean {
  return assets.some((asset) => asset.ideal_class_percent > 0);
}
