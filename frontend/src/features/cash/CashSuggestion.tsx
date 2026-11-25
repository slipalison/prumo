import { useCash } from "../../api/cashQueries";
import type { Cash, CashMovement } from "../../api/cashTypes";
import { formatIsoDate, formatIsoDay } from "../../lib/income";
import { plural } from "../../lib/format";
import { useMoney } from "../../lib/privacy";

type CashSuggestionProps = {
  /** O valor do aporte digitado. */
  amount: number;
  /** Quanto do caixa já está incluído no valor (`0` = nada). */
  included: number;
  /** O novo valor do aporte e quanto dele é caixa. */
  onChange: (amount: number, included: number) => void;
};

const cents = (value: number) => Math.round(value * 100) / 100;

/**
 * "+ R$ 812 em caixa — incluir?". O caixa é o que os proventos e as vendas
 * deixaram e ainda não foi aportado. Só soma ao valor de entrada, se a pessoa
 * aceitar: o motor do aporte não muda. O detalhe diz de onde o saldo veio.
 */
export function CashSuggestion({ amount, included, onChange }: CashSuggestionProps) {
  const cash = useCash();
  const money = useMoney();
  const balance = cash.data?.balance ?? 0;
  // Registrado o aporte, o campo zera e o caixa já baixou: a sugestão volta a oferecer.
  const active = included > 0 && amount >= included;
  if (!cash.data || (balance <= 0 && !active)) return null;

  return (
    <div className="cash-suggestion" role="group" aria-label="Caixa disponível para aportar">
      <p className="cash-suggestion__line">
        {active ? (
          <>
            <span>{money.brl(included)} do caixa incluídos no aporte.</span>
            <button type="button" className="link-btn" onClick={() => onChange(cents(Math.max(0, amount - included)), 0)}>
              Tirar
            </button>
          </>
        ) : (
          <>
            <span>+ {money.brl(balance)} em caixa</span>
            <button type="button" className="link-btn" onClick={() => onChange(cents(amount + balance), balance)}>
              Incluir
            </button>
          </>
        )}
      </p>
      <CashDetail cash={cash.data} />
    </div>
  );
}

const KIND_LABEL: Record<CashMovement["kind"], string> = {
  income: "Provento",
  sale: "Venda",
  contribution: "Aporte",
};

/** De onde o saldo veio: proventos, vendas e o que os aportes já usaram. */
function CashDetail({ cash }: { cash: Cash }) {
  const money = useMoney();
  return (
    <details className="cash-detail">
      <summary>De onde vem esse caixa</summary>
      <dl className="cash-detail__sources">
        <div>
          <dt>
            Proventos recebidos ({cash.income.count} {plural(cash.income.count, ["pagamento", "pagamentos"])})
          </dt>
          <dd>{money.brl(cash.income.amount)}</dd>
        </div>
        <div>
          <dt>
            Vendas, líquido de taxas ({cash.sales.count} {plural(cash.sales.count, ["venda", "vendas"])})
          </dt>
          <dd>{money.brl(cash.sales.amount)}</dd>
        </div>
        <div>
          <dt>Já usado em aportes</dt>
          <dd>− {money.brl(cash.used)}</dd>
        </div>
        {cash.adjustment > 0 && (
          <div>
            <dt>Aporte que passou do caixa depois de um provento ser apagado</dt>
            <dd>+ {money.brl(cash.adjustment)}</dd>
          </div>
        )}
      </dl>
      <p className="field__hint">
        Conta desde {formatIsoDate(cash.since)}. Aportou dinheiro de fora? Digite o valor total: só o que você incluir daqui sai do
        caixa.
      </p>
      {cash.recent.length > 0 && (
        <ul className="cash-detail__recent">
          {cash.recent.map((movement, index) => (
            <li key={`${movement.date}-${movement.kind}-${movement.label}-${index}`}>
              <span>
                {formatIsoDay(movement.date)} · {KIND_LABEL[movement.kind]} · {movement.label}
              </span>
              <strong>
                {movement.kind === "contribution" ? "− " : "+ "}
                {money.brl(movement.amount)}
              </strong>
            </li>
          ))}
        </ul>
      )}
    </details>
  );
}
