import { useUnusualIncome } from "../../api/agenda";
import { UnusualBadge } from "./AgendaEventItem";

/**
 * O selo de "fora do padrão" num provento a receber da Renda: o valor por cota
 * anunciado se afastou do habitual do ativo (docs/agenda.md). Uma consulta só
 * para a lista inteira (o cache do react-query); sem selo, nada aparece.
 */
export function UnusualIncomeTag({ eventId }: Readonly<{ eventId: string }>) {
  const unusual = useUnusualIncome();
  const found = unusual.data?.items.find((item) => item.income_ids.includes(eventId));
  if (!found) return null;
  return (
    <>
      {" "}
      <UnusualBadge mark={found} installments={found.installments} />
    </>
  );
}
