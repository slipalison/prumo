import type { Asset, AssetClass } from "../../api/types";
import { Sheet } from "../../components/Sheet";
import { useToast } from "../../components/toastContext";
import { assetName } from "../../lib/rating";
import { AssetForm } from "./AssetForm";

type AddAssetSheetProps = {
  open: boolean;
  initialClass?: AssetClass | undefined;
  onClose: () => void;
  onCreated: (asset: Asset) => void;
};

export function AddAssetSheet({ open, initialClass, onClose, onCreated }: AddAssetSheetProps) {
  const toast = useToast();
  return (
    <Sheet open={open} onClose={onClose} title="Novo ativo" description="Digite o ticker. Pode ser um ativo que você ainda não comprou: com a nota, ele já entra no aporte.">
      <AssetForm
        key={initialClass ?? "any"}
        initialClass={initialClass}
        onCreated={(asset) => {
          toast({ tone: "success", message: `${assetName(asset)} entrou na carteira.` });
          onCreated(asset);
        }}
      />
    </Sheet>
  );
}
