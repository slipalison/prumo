import { useEffect, useId, useState, type FormEvent } from "react";
import { errorMessage } from "../../api/client";
import { useCreateObjective, useUpdateObjective } from "../../api/objectivesQueries";
import type { Objective, ObjectiveKind, ObjectivesResponse } from "../../api/objectivesTypes";
import type { AssetClass } from "../../api/types";
import { ASSET_CLASSES } from "../../api/types";
import { MagneticButton } from "../../components/MagneticButton";
import { MoneyInput } from "../../components/MoneyInput";
import { Sheet } from "../../components/Sheet";
import { useToast } from "../../components/toastContext";
import { CLASS_META } from "../../lib/assetClasses";
import { formatPercent, parseDecimal } from "../../lib/format";
import { objectiveName, percentToField, RETURN_DEFAULT_TEXT } from "../../lib/objectives";
import { useMoney } from "../../lib/privacy";

const MIN_RETURN = -50;
const MAX_RETURN = 100;

type ObjectiveSheetProps = {
  open: boolean;
  onClose: () => void;
  /** Sem objetivo, a folha cria um; com ele, troca a meta e as premissas. */
  objective: Objective | null;
  defaults: ObjectivesResponse["defaults"];
};

/** Texto do campo de rentabilidade → número, ou o motivo de não servir. */
function parseReturn(text: string): { value: number | null; error: string | null } {
  const value = parseDecimal(text);
  if (value === null || value < MIN_RETURN || value > MAX_RETURN) {
    return { value: null, error: `Use um número de ${MIN_RETURN}% a ${MAX_RETURN}% ao ano.` };
  }
  return { value, error: null };
}

/**
 * Criar ou editar um objetivo. A meta é o único campo obrigatório; o aporte
 * mensal e a rentabilidade começam no padrão (a média do livro e a do
 * Desempenho), e quem quiser trocar desmarca a caixa e digita o seu.
 */
export function ObjectiveSheet({ open, onClose, objective, defaults }: ObjectiveSheetProps) {
  const ids = {
    kind: useId(),
    class: useId(),
    target: useId(),
    ownContribution: useId(),
    contribution: useId(),
    ownRate: useId(),
    rate: useId(),
  };
  const editing = objective !== null;
  const [kind, setKind] = useState<ObjectiveKind>("net_worth");
  const [assetClass, setAssetClass] = useState<AssetClass>("br_reits");
  const [target, setTarget] = useState(0);
  const [ownContribution, setOwnContribution] = useState(false);
  const [contribution, setContribution] = useState(0);
  const [ownRate, setOwnRate] = useState(false);
  const [rate, setRate] = useState("");
  const create = useCreateObjective();
  const update = useUpdateObjective();
  const toast = useToast();
  const money = useMoney();

  // Cada abertura parte do que está salvo, não do que ficou digitado na anterior.
  useEffect(() => {
    if (!open) return;
    setKind(objective?.kind ?? "net_worth");
    setAssetClass(objective?.asset_class ?? "br_reits");
    setTarget(objective?.target ?? 0);
    setOwnContribution((objective?.monthly_contribution ?? null) !== null);
    setContribution(objective?.monthly_contribution ?? 0);
    setOwnRate((objective?.expected_return ?? null) !== null);
    setRate(percentToField(objective?.expected_return ?? null));
    create.reset();
    update.reset();
    // `create` e `update` mudam de identidade a cada render; só a abertura importa.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, objective]);

  const rateCheck = ownRate ? parseReturn(rate) : { value: null, error: null };
  const valid = target > 0 && !rateCheck.error;
  const pending = create.isPending || update.isPending;
  const error = create.error ?? update.error;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!valid || pending) return;
    const done = (message: string) => ({
      onSuccess: () => {
        toast({ tone: "success", message });
        onClose();
      },
    });
    if (objective) {
      update.mutate(
        {
          id: objective.id,
          body: {
            target,
            monthly_contribution: ownContribution ? contribution : null,
            expected_return: ownRate ? rateCheck.value : null,
          },
        },
        done("Objetivo salvo."),
      );
      return;
    }
    create.mutate(
      {
        kind,
        ...(kind === "class_value" ? { asset_class: assetClass } : {}),
        target,
        ...(ownContribution ? { monthly_contribution: contribution } : {}),
        ...(ownRate && rateCheck.value !== null ? { expected_return: rateCheck.value } : {}),
      },
      done("Objetivo criado."),
    );
  };

  return (
    <Sheet
      open={open}
      onClose={onClose}
      title={editing ? "Editar objetivo" : "Novo objetivo"}
      description="A data estimada sai de uma conta com duas premissas. Elas ficam sempre à vista e você pode trocá-las."
    >
      <form className="asset-form" onSubmit={submit} noValidate>
        {editing ? (
          <p className="objective-form__fixed">
            <strong>{objectiveName(objective)}</strong>
            <span className="field__hint"> O tipo e a classe não mudam. Para trocar, crie outro objetivo.</span>
          </p>
        ) : (
          <>
            <div className="field">
              <label htmlFor={ids.kind} className="field__label">
                O que medir
              </label>
              <select
                id={ids.kind}
                className="text-input"
                value={kind}
                onChange={(event) => setKind(event.target.value as ObjectiveKind)}
              >
                <option value="net_worth">Patrimônio total</option>
                <option value="class_value">Valor numa classe</option>
              </select>
            </div>
            {kind === "class_value" && (
              <div className="field">
                <label htmlFor={ids.class} className="field__label">
                  Classe
                </label>
                <select
                  id={ids.class}
                  className="text-input"
                  value={assetClass}
                  onChange={(event) => setAssetClass(event.target.value as AssetClass)}
                >
                  {ASSET_CLASSES.map((c) => (
                    <option key={c} value={c}>
                      {CLASS_META[c].label}
                    </option>
                  ))}
                </select>
              </div>
            )}
          </>
        )}

        <MoneyInput
          id={ids.target}
          label="Meta"
          value={target}
          onChange={setTarget}
          focusOnMount={!money.hidden}
        />

        <fieldset className="objective-form__premise">
          <legend className="field__label">Aporte por mês</legend>
          <div className="switch-row">
            <label htmlFor={ids.ownContribution} className="switch-row__label">
              Informar o meu aporte
              <span className="field__hint">
                {ownContribution
                  ? "A conta usa o valor abaixo."
                  : "A conta usa a média real das suas compras nos últimos 12 meses, do livro."}
              </span>
            </label>
            <input
              id={ids.ownContribution}
              type="checkbox"
              role="switch"
              className="switch"
              checked={ownContribution}
              onChange={(event) => setOwnContribution(event.target.checked)}
            />
          </div>
          {ownContribution && (
            <MoneyInput id={ids.contribution} label="Aporte por mês" value={contribution} onChange={setContribution} />
          )}
        </fieldset>

        <fieldset className="objective-form__premise">
          <legend className="field__label">Rentabilidade ao ano</legend>
          <div className="switch-row">
            <label htmlFor={ids.ownRate} className="switch-row__label">
              Informar a minha rentabilidade
              <span className="field__hint">
                {ownRate
                  ? "A conta usa o valor abaixo."
                  : `A conta usa ${formatPercent(defaults.annual_return_percent, 2)} ao ano: ${RETURN_DEFAULT_TEXT[defaults.return_source]}.`}
              </span>
            </label>
            <input
              id={ids.ownRate}
              type="checkbox"
              role="switch"
              className="switch"
              checked={ownRate}
              onChange={(event) => {
                setOwnRate(event.target.checked);
                if (event.target.checked && rate === "") setRate(percentToField(defaults.annual_return_percent));
              }}
            />
          </div>
          {ownRate && (
            <div className="field">
              <label htmlFor={ids.rate} className="field__label">
                Rentabilidade esperada (% ao ano)
              </label>
              <input
                id={ids.rate}
                className="text-input"
                inputMode="decimal"
                value={rate}
                aria-invalid={rateCheck.error ? true : undefined}
                aria-describedby={rateCheck.error ? `${ids.rate}-error` : undefined}
                onChange={(event) => setRate(event.target.value)}
              />
              {rateCheck.error && (
                <p id={`${ids.rate}-error`} className="rule-row__error" role="alert">
                  {rateCheck.error}
                </p>
              )}
            </div>
          )}
        </fieldset>

        {error && (
          <p className="form-error" role="alert">
            {errorMessage(error)}
          </p>
        )}
        <MagneticButton type="submit" disabled={!valid} loading={pending}>
          {editing ? "Salvar objetivo" : "Criar objetivo"}
        </MagneticButton>
      </form>
    </Sheet>
  );
}
