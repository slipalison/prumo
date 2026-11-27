import { IconClose } from "../../components/icons";

/**
 * O selo do filtro por pagador: diz quem filtra e limpa com um clique. Fica no
 * cabeçalho dos painéis filtrados (Quem pagou, Próximos pagamentos).
 */
export function FilterChip({ label, onClear }: Readonly<{ label: string; onClear: () => void }>) {
  return (
    <button type="button" className="income-filter" aria-label={`Limpar o filtro por ${label}`} onClick={onClear}>
      <span>
        Filtro: <strong>{label}</strong>
      </span>
      <IconClose aria-hidden="true" width={12} height={12} />
    </button>
  );
}
