import { useRef } from "react";
import { errorMessage } from "../../api/client";
import { useContribute } from "../../api/queries";
import type { Contribution, Plan } from "../../api/types";
import { AssetIcon } from "../../components/AssetIcon";
import { MagneticButton } from "../../components/MagneticButton";
import { Sheet } from "../../components/Sheet";
import { celebrate, originFrom } from "../../lib/celebrate";
import { plural } from "../../lib/format";
import { useMoney } from "../../lib/privacy";
import { assetName } from "../../lib/rating";
import { purchaseLine } from "../../lib/planText";
import { contributionBody, markValue, markedItems, type Marks } from "./marks";

type ContributionSheetProps = {
  open: boolean;
  plan: Plan;
  /** O que a pessoa marcou como comprado; só isso é registrado. */
  marks: Marks;
  /** Quanto do valor digitado era caixa; o aporte baixa dele o que couber (`cashUsedFor`). */
  cashIncluded: number;
  onClose: () => void;
  onRegistered: (contribution: Contribution) => void;
};

/**
 * "Registrar N comprados": a última conferência antes de gravar. Lista só os
 * ativos marcados, com o que foi comprado e o total; quem ficou de fora não
 * entra. A quantidade e o preço se corrigem nos cartões (Editar), não aqui.
 */
export function ContributionSheet({ open, plan, marks, cashIncluded, onClose, onRegistered }: Readonly<ContributionSheetProps>) {
  return (
    <Sheet
      open={open}
      onClose={onClose}
      title="Registrar aporte"
      description="Confira o que você comprou. As quantidades entram na carteira."
    >
      {open && <ContributionForm plan={plan} marks={marks} cashIncluded={cashIncluded} onRegistered={onRegistered} />}
    </Sheet>
  );
}

type FormProps = Pick<ContributionSheetProps, "plan" | "marks" | "cashIncluded" | "onRegistered">;

function ContributionForm({ plan, marks, cashIncluded, onRegistered }: Readonly<FormProps>) {
  const money = useMoney();
  const contribute = useContribute();
  const buttonRef = useRef<HTMLDivElement>(null);

  const body = contributionBody(plan, marks, cashIncluded);
  const chosen = markedItems(plan, marks);
  const left = plan.items.length - chosen.length;

  const register = () => {
    contribute.mutate(body, {
      onSuccess: (contribution) => {
        celebrate(originFrom(buttonRef.current));
        onRegistered(contribution);
      },
    });
  };

  return (
    <div className="contribution-form">
      <ul className="contribution-form__list">
        {chosen.map((item) => {
          const mark = marks[item.asset_id];
          if (!mark) return null;
          return (
            <li key={item.asset_id} className="contribution-form__row">
              <AssetIcon symbol={item.symbol} name={item.display_name} assetClass={item.asset_class} treasury={item.treasury} />
              <span className="contribution-form__text">
                <strong>{assetName(item)}</strong>
                <span className="field__hint">{purchaseLine(item, mark, money)}</span>
              </span>
              <strong className="contribution-form__value">{money.brl(markValue(mark))}</strong>
            </li>
          );
        })}
      </ul>

      <div className="contribution-form__total">
        <span>Total</span>
        <strong>{money.brl(body.amount)}</strong>
      </div>
      {(body.cash_used ?? 0) > 0 && <p className="field__hint">{money.brl(body.cash_used ?? 0)} deste aporte saem do caixa.</p>}
      {left > 0 && (
        <p className="field__hint">
          {left} {plural(left, ["ativo ficou", "ativos ficaram"])} de fora deste registro.
        </p>
      )}

      {contribute.isError && (
        <p className="form-error" role="alert">
          {errorMessage(contribute.error)}
        </p>
      )}

      <div ref={buttonRef}>
        <MagneticButton onClick={register} loading={contribute.isPending} disabled={chosen.length === 0} className="wide">
          Registrar aporte
        </MagneticButton>
      </div>
    </div>
  );
}
