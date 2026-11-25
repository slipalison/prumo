import { useId, useRef, useState } from "react";
import { errorMessage } from "../../api/client";
import { useContribute } from "../../api/queries";
import type { Contribution, Plan, PlanItem } from "../../api/types";
import { MagneticButton } from "../../components/MagneticButton";
import { MoneyInput } from "../../components/MoneyInput";
import { QuantityInput } from "../../components/QuantityInput";
import { Sheet } from "../../components/Sheet";
import { nounOf, unitOf } from "../../lib/assetClasses";
import { celebrate, originFrom } from "../../lib/celebrate";
import { useMoney } from "../../lib/privacy";
import { assetName } from "../../lib/rating";

type ContributionSheetProps = {
  open: boolean;
  plan: Plan;
  /** Quanto do aporte veio do caixa; baixa do saldo ao registrar. */
  cashUsed: number;
  onClose: () => void;
  onRegistered: (contribution: Contribution) => void;
};

/**
 * "Já comprei": confirma o que foi comprado de fato. Vem preenchido com o
 * plano; se a corretora executou diferente, ajusta aqui antes de registrar.
 */
export function ContributionSheet({ open, plan, cashUsed, onClose, onRegistered }: ContributionSheetProps) {
  return (
    <Sheet
      open={open}
      onClose={onClose}
      title="Registrar aporte"
      description="Confira o que você comprou. As quantidades entram na carteira."
    >
      {open && <ContributionForm plan={plan} cashUsed={cashUsed} onRegistered={onRegistered} />}
    </Sheet>
  );
}

function ContributionForm({ plan, cashUsed, onRegistered }: { plan: Plan; cashUsed: number; onRegistered: (c: Contribution) => void }) {
  const baseId = useId();
  const money = useMoney();
  const [quantities, setQuantities] = useState<Record<string, number>>(() =>
    Object.fromEntries(plan.items.map((item) => [item.asset_id, item.quantity])),
  );
  const contribute = useContribute();
  const buttonRef = useRef<HTMLDivElement>(null);

  const valueOf = (item: PlanItem) => (quantities[item.asset_id] ?? 0) * item.unit_price;
  const total = plan.items.reduce((acc, item) => acc + valueOf(item), 0);
  const chosen = plan.items.filter((item) => (quantities[item.asset_id] ?? 0) > 0);

  const register = () => {
    contribute.mutate(
      {
        amount: plan.amount,
        ...(cashUsed > 0 ? { cash_used: cashUsed } : {}),
        items: chosen.map((item) => ({
          asset_id: item.asset_id,
          quantity: quantities[item.asset_id] ?? 0,
          unit_price: item.unit_price,
        })),
      },
      {
        onSuccess: (contribution) => {
          celebrate(originFrom(buttonRef.current));
          onRegistered(contribution);
        },
      },
    );
  };

  return (
    <div className="contribution-form">
      <ul className="contribution-form__list">
        {plan.items.map((item, index) => {
          const unit = unitOf(item.asset_class, item.listed_on_b3, item.treasury);
          const noun = nounOf(item.asset_class, item.listed_on_b3, item.treasury);
          const id = `${baseId}-${index}`;
          const set = (value: number) => setQuantities((current) => ({ ...current, [item.asset_id]: value }));
          return (
            <li key={item.asset_id} className="contribution-form__row">
              <div className="contribution-form__id">
                <strong>{assetName(item)}</strong>
                <span className="field__hint">{money.brl(valueOf(item))}</span>
              </div>
              {unit === "value" ? (
                <MoneyInput id={id} label="Valor aplicado" value={quantities[item.asset_id] ?? 0} onChange={set} />
              ) : (
                <QuantityInput
                  id={id}
                  label={unit === "fraction" ? "Quantidade" : noun[1].replace(/^./, (c) => c.toUpperCase())}
                  value={quantities[item.asset_id] ?? 0}
                  onChange={set}
                  fractional={unit === "fraction"}
                />
              )}
            </li>
          );
        })}
      </ul>

      <div className="contribution-form__total">
        <span>Total</span>
        <strong>{money.brl(total)}</strong>
      </div>
      {cashUsed > 0 && <p className="field__hint">{money.brl(cashUsed)} deste aporte saem do caixa.</p>}

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
