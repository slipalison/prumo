import type { Asset, Portfolio } from "../../api/types";
import { EditAssetSheet } from "./EditAssetSheet";

/** O ativo a avaliar e a dica do topo; `null` com a folha fechada. */
export type RatingTarget = { asset: Asset; intro?: string } | null;

type RateAssetSheetProps = {
  target: RatingTarget;
  portfolio: Portfolio;
  onClose: () => void;
  onSaved?: (asset: Asset) => void;
};

/**
 * A folha de edição aberta a partir de fora da Carteira (primeiro uso, aviso
 * do plano). Usa a versão mais nova do ativo quando a carteira já a tem — o
 * ativo recém-criado ainda pode não ter chegado na lista.
 */
export function RateAssetSheet({ target, portfolio, onClose, onSaved }: Readonly<RateAssetSheetProps>) {
  const asset = target ? (portfolio.assets.find((a) => a.id === target.asset.id) ?? target.asset) : null;
  return (
    <EditAssetSheet asset={asset} portfolio={portfolio} intro={target?.intro} onClose={onClose} onSaved={onSaved} />
  );
}
