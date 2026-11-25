import { useState } from "react";
import { useCashSuggestion } from "../../api/incomeQueries";
import { plural } from "../../lib/format";
import { useMoney } from "../../lib/privacy";

type CashSuggestionProps = {
  /** O valor do aporte digitado. */
  amount: number;
  onChange: (amount: number) => void;
};

/**
 * "+ R$ 812 de proventos recebidos desde o último aporte — incluir?". Só soma
 * ao valor de entrada, se a pessoa aceitar: o motor do aporte não muda.
 */
export function CashSuggestion({ amount, onChange }: CashSuggestionProps) {
  const cash = useCashSuggestion();
  const money = useMoney();
  const [included, setIncluded] = useState(false);
  const suggested = cash.data?.amount ?? 0;
  if (!cash.data || suggested <= 0) return null;
  // Registrado o aporte, o campo zera: a sugestão volta a oferecer.
  const active = included && amount >= suggested;

  return (
    <p className="cash-suggestion" role="group" aria-label="Proventos recebidos desde o último aporte">
      {active ? (
        <>
          <span>{money.brl(suggested)} de proventos incluídos no aporte.</span>
          <button
            type="button"
            className="link-btn"
            onClick={() => {
              setIncluded(false);
              onChange(Math.max(0, Math.round((amount - suggested) * 100) / 100));
            }}
          >
            Tirar
          </button>
        </>
      ) : (
        <>
          <span>
            + {money.brl(suggested)} de proventos recebidos desde o último aporte ({cash.data.count}{" "}
            {plural(cash.data.count, ["pagamento", "pagamentos"])}).
          </span>
          <button
            type="button"
            className="link-btn"
            onClick={() => {
              setIncluded(true);
              onChange(Math.round((amount + suggested) * 100) / 100);
            }}
          >
            Incluir
          </button>
        </>
      )}
    </p>
  );
}
