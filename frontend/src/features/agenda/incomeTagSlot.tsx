import type { ReactNode } from "react";
import type { IncomeEvent } from "../../api/incomeTypes";
import { UnusualIncomeTag } from "./UnusualIncomeTag";

/** O que a lista de "Próximos pagamentos" da Renda monta ao lado de cada provento. */
export const unusualIncomeTag = (event: IncomeEvent): ReactNode => <UnusualIncomeTag eventId={event.id} />;
