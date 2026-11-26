import { useId, useState } from "react";
import { errorMessage } from "../../api/client";
import type { Carryforward, CarryforwardValues } from "../../api/taxTypes";
import { Panel } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { MoneyInput } from "../../components/MoneyInput";
import { useToast } from "../../components/toastContext";
import { cx } from "../../lib/cx";
import { useMoney } from "../../lib/privacy";
import { useSaveCarryforward } from "./useTax";

const CARRY_FIELDS: Array<{ key: keyof CarryforwardValues; label: string }> = [
  { key: "common_loss", label: "Prejuízo em ações, ETF e BDR" },
  { key: "day_trade_loss", label: "Prejuízo em day trade" },
  { key: "real_estate_fund_loss", label: "Prejuízo em FII" },
  { key: "agribusiness_fund_loss", label: "Prejuízo em Fiagro" },
  { key: "pending_exchange_darf", label: "DARF 6015 abaixo do mínimo, não pago" },
  { key: "pending_capital_gain_darf", label: "DARF 4600 abaixo do mínimo, não pago" },
];

function valuesOf(carry: Carryforward): CarryforwardValues {
  return {
    common_loss: carry.common_loss,
    day_trade_loss: carry.day_trade_loss,
    real_estate_fund_loss: carry.real_estate_fund_loss,
    agribusiness_fund_loss: carry.agribusiness_fund_loss,
    pending_exchange_darf: carry.pending_exchange_darf,
    pending_capital_gain_darf: carry.pending_capital_gain_darf,
  };
}

/**
 * O saldo de prejuízo (e de DARF abaixo do mínimo) de antes do Prumo, como
 * estava em 1º de janeiro do primeiro ano apurado. Ele muda a conta de todos
 * os anos.
 */
export function CarryforwardPanel({ carry }: { carry: Carryforward }) {
  const money = useMoney();
  const baseId = useId();
  const [editing, setEditing] = useState(false);
  const [year, setYear] = useState(carry.year);
  const [values, setValues] = useState<CarryforwardValues>(() => valuesOf(carry));
  const save = useSaveCarryforward();
  const toast = useToast();
  const filled = CARRY_FIELDS.filter((field) => carry[field.key] > 0);

  const open = () => {
    setYear(carry.year);
    setValues(valuesOf(carry));
    setEditing(true);
  };
  const submit = () =>
    save.mutate(
      { year, ...values },
      {
        onSuccess: () => {
          toast({ tone: "success", message: "Saldo de antes do Prumo salvo." });
          setEditing(false);
        },
      },
    );

  return (
    <Panel
      title="Saldo de antes do Prumo"
      info="O prejuízo a compensar e o DARF abaixo do mínimo ainda não pago, como estavam em 1º de janeiro do primeiro ano apurado. Estão na sua última declaração (Renda Variável) ou no controle do contador."
      infoLabel="De onde vem o saldo de antes do Prumo"
    >
      {!editing ? (
        <>
          {carry.informed && filled.length > 0 ? (
            <ul className="tax-facts" aria-label={`Saldo em 1º de janeiro de ${carry.year}`}>
              {filled.map((field) => (
                <li key={field.key}>
                  {field.label}: {money.brl(carry[field.key])}
                </li>
              ))}
            </ul>
          ) : (
            <p className="tax-facts">
              {carry.informed ? "Saldo informado: nada a compensar" : "Nada informado: a conta começa do zero"} em 1º de janeiro de{" "}
              {carry.year}.
            </p>
          )}
          <div className="tax-carry__actions">
            <MagneticButton variant="ghost" magnetic={false} onClick={open}>
              {carry.informed ? "Editar saldo" : "Informar saldo"}
            </MagneticButton>
          </div>
        </>
      ) : (
        <div className="tax-carry">
          <div className="chips" role="radiogroup" aria-label="O saldo vale em 1º de janeiro de">
            {carry.covered_years.map((option) => (
              <button
                key={option}
                type="button"
                role="radio"
                aria-checked={option === year}
                className={cx("chip chip--quick", option === year && "is-active")}
                onClick={() => setYear(option)}
              >
                1º/1/{option}
              </button>
            ))}
          </div>
          <div className="tax-carry__fields">
            {CARRY_FIELDS.map((field) => (
              <MoneyInput
                key={field.key}
                id={`${baseId}-${field.key}`}
                label={field.label}
                value={values[field.key]}
                onChange={(value) => setValues((current) => ({ ...current, [field.key]: value }))}
              />
            ))}
          </div>
          {save.isError && (
            <p className="form-error" role="alert">
              {errorMessage(save.error)}
            </p>
          )}
          <div className="tax-carry__actions">
            <MagneticButton onClick={submit} loading={save.isPending}>
              Salvar saldo
            </MagneticButton>
            <MagneticButton variant="ghost" magnetic={false} onClick={() => setEditing(false)}>
              Cancelar
            </MagneticButton>
          </div>
        </div>
      )}
    </Panel>
  );
}
