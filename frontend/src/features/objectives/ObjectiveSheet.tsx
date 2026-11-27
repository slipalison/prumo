import { useEffect, useId, useState, type FormEvent } from "react";
import { errorMessage } from "../../api/client";
import { useCreateObjective, useUpdateObjective } from "../../api/objectivesQueries";
import type {
  CreateObjectiveRequest,
  IncomeMode,
  Objective,
  ObjectiveKind,
  ObjectivesDefaults,
  UpdateObjectiveRequest,
} from "../../api/objectivesTypes";
import type { AssetClass } from "../../api/types";
import { ASSET_CLASSES } from "../../api/types";
import { MagneticButton } from "../../components/MagneticButton";
import { MoneyInput } from "../../components/MoneyInput";
import { Sheet } from "../../components/Sheet";
import { useToast } from "../../components/toastContext";
import { CLASS_META } from "../../lib/assetClasses";
import { formatPercent, parseDecimal } from "../../lib/format";
import { formatPoints, inflationSourceText, objectiveName, percentToField, RETURN_DEFAULT_TEXT } from "../../lib/objectives";
import { useMoney } from "../../lib/privacy";

/** As taxas que a folha aceita, em % ao ano (os mesmos limites do servidor). */
type RateKey = "real_return" | "inflation" | "income_growth" | "income_yield" | "scenario_spread";

const RATES: Record<RateKey, { label: string; min: number; max: number }> = {
  real_return: { label: "Rentabilidade acima da inflação (% ao ano)", min: -50, max: 50 },
  inflation: { label: "Inflação (% ao ano)", min: -10, max: 50 },
  income_growth: { label: "Crescimento dos proventos acima da inflação (% ao ano)", min: -50, max: 50 },
  income_yield: { label: "Rendimento do dinheiro novo (% ao ano)", min: 0, max: 50 },
  scenario_spread: { label: "Amplitude dos cenários (pontos ao ano)", min: 0, max: 20 },
};

type RateFields = Record<RateKey, string>;

const EMPTY_RATES: RateFields = { real_return: "", inflation: "", income_growth: "", income_yield: "", scenario_spread: "" };

/** Texto do campo → número, `null` (vazio = padrão) ou o motivo de não servir. */
function parseRate(key: RateKey, text: string): { value: number | null; error: string | null } {
  if (text.trim() === "") return { value: null, error: null };
  const { min, max } = RATES[key];
  const value = parseDecimal(text);
  if (value === null || value < min || value > max) {
    return { value: null, error: `Use um número de ${min} a ${max}.` };
  }
  return { value, error: null };
}

type ObjectiveSheetProps = {
  open: boolean;
  onClose: () => void;
  /** Sem objetivo, a folha cria um; com ele, troca a meta e as premissas. */
  objective: Objective | null;
  defaults: ObjectivesDefaults;
  /** Já existe a meta de renda (uma por pessoa): a opção fica desligada. */
  hasIncomeGoal: boolean;
};

/**
 * Criar ou editar um objetivo. A meta é o único campo obrigatório; cada
 * premissa começa no padrão (a folha diz qual e de onde vem) e quem quiser
 * trocar digita a sua. Campo vazio volta ao padrão.
 */
export function ObjectiveSheet({ open, onClose, objective, defaults, hasIncomeGoal }: Readonly<ObjectiveSheetProps>) {
  const ids = {
    kind: useId(),
    class: useId(),
    mode: useId(),
    target: useId(),
    ownContribution: useId(),
    contribution: useId(),
    reinvest: useId(),
    rate: useId(),
  };
  const editing = objective !== null;
  const [kind, setKind] = useState<ObjectiveKind>("net_worth");
  const [assetClass, setAssetClass] = useState<AssetClass>("br_reits");
  const [mode, setMode] = useState<IncomeMode>("average");
  const [target, setTarget] = useState(0);
  const [ownContribution, setOwnContribution] = useState(false);
  const [contribution, setContribution] = useState(0);
  const [keepIncome, setKeepIncome] = useState(false);
  const [rates, setRates] = useState<RateFields>(EMPTY_RATES);
  const create = useCreateObjective();
  const update = useUpdateObjective();
  const toast = useToast();
  const money = useMoney();

  // Cada abertura parte do que está salvo, não do que ficou digitado na anterior.
  useEffect(() => {
    if (!open) return;
    const p = objective?.premises;
    setKind(objective?.kind ?? "net_worth");
    setAssetClass(objective?.asset_class ?? "br_reits");
    setMode(objective?.income_mode ?? "average");
    setTarget(objective?.target ?? 0);
    setOwnContribution((p?.monthly_contribution ?? null) !== null);
    setContribution(p?.monthly_contribution ?? 0);
    setKeepIncome(p?.reinvest_income === false);
    setRates({
      real_return: percentToField(p?.real_return ?? null),
      inflation: percentToField(p?.inflation ?? null),
      income_growth: percentToField(p?.income_growth ?? null),
      income_yield: percentToField(p?.income_yield ?? null),
      scenario_spread: percentToField(p?.scenario_spread ?? null),
    });
    create.reset();
    update.reset();
    // `create` e `update` mudam de identidade a cada render; só a abertura importa.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, objective]);

  const income = kind === "monthly_income";
  const shown: RateKey[] = income
    ? ["inflation", "income_growth", "income_yield", "scenario_spread"]
    : ["real_return", "inflation", "income_yield", "scenario_spread"];
  const checks = Object.fromEntries(shown.map((key) => [key, parseRate(key, rates[key])])) as Partial<
    Record<RateKey, ReturnType<typeof parseRate>>
  >;
  const valid = target > 0 && shown.every((key) => !checks[key]?.error);
  const pending = create.isPending || update.isPending;
  const error = create.error ?? update.error;

  const defaultHint: Record<RateKey, string> = {
    real_return: `${formatPercent(defaults.real_return_percent, 2)}: ${RETURN_DEFAULT_TEXT[defaults.return_source]}`,
    inflation: `${formatPercent(defaults.inflation_percent, 2)}: ${inflationSourceText(defaults.inflation_source, defaults)}`,
    income_growth: `${formatPercent(defaults.income_growth_percent, 2)}: sem crescimento real`,
    income_yield: `${formatPercent(defaults.income_yield_percent, 2)}: o da carteira de hoje (num objetivo de classe, o da classe)`,
    scenario_spread: `${formatPoints(defaults.scenario_spread_percent)} ao ano para cada lado`,
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!valid || pending) return;
    const done = (message: string) => ({
      onSuccess: () => {
        toast({ tone: "success", message });
        onClose();
      },
    });
    const value = (key: RateKey) => checks[key]?.value ?? null;
    if (objective) {
      const body: UpdateObjectiveRequest = {
        target,
        monthly_contribution: ownContribution ? contribution : null,
        reinvest_income: keepIncome ? false : null,
        ...Object.fromEntries(shown.map((key) => [key, value(key)])),
        ...(income ? { income_mode: mode } : {}),
      };
      update.mutate({ id: objective.id, body }, done("Objetivo salvo."));
      return;
    }
    const informed = Object.fromEntries(
      shown.flatMap((key) => {
        const v = value(key);
        return v === null ? [] : [[key, v]];
      }),
    );
    const body: CreateObjectiveRequest = {
      kind,
      target,
      ...(kind === "class_value" ? { asset_class: assetClass } : {}),
      ...(income ? { income_mode: mode } : {}),
      ...(ownContribution ? { monthly_contribution: contribution } : {}),
      ...(keepIncome ? { reinvest_income: false } : {}),
      ...informed,
    };
    create.mutate(body, done("Objetivo criado."));
  };

  return (
    <Sheet
      open={open}
      onClose={onClose}
      title={editing ? "Editar objetivo" : "Novo objetivo"}
      description="A data sai de uma conta com premissas. Cada uma começa no padrão, que a folha diz qual é; troque as que quiser."
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
                <option value="monthly_income" disabled={hasIncomeGoal}>
                  {hasIncomeGoal ? "Renda mensal (já existe: edite a sua)" : "Renda mensal"}
                </option>
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

        {income && (
          <div className="field">
            <label htmlFor={ids.mode} className="field__label">
              Como a renda conta
            </label>
            <select id={ids.mode} className="text-input" value={mode} onChange={(event) => setMode(event.target.value as IncomeMode)}>
              <option value="average">A média por mês</option>
              <option value="floor">Todo mês: o menor mês de 12</option>
            </select>
            <span className="field__hint">
              {mode === "floor"
                ? "A meta só conta quando o pior mês de 12 seguidos chega ao valor."
                : "A meta conta pela média: um mês fraco é compensado por um forte."}
            </span>
          </div>
        )}

        <MoneyInput
          id={ids.target}
          label={income ? "Meta por mês" : "Meta"}
          value={target}
          onChange={setTarget}
          focusOnMount={!money.hidden}
        />

        <fieldset className="objective-form__premise">
          <legend className="field__label">Aporte por mês, em reais de hoje</legend>
          <div className="switch-row">
            <label htmlFor={ids.ownContribution} className="switch-row__label">
              Informar o meu aporte{" "}
              <span className="field__hint">
                {ownContribution
                  ? "A conta usa o valor abaixo, corrigido pela inflação a cada mês."
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
          <legend className="field__label">Proventos</legend>
          <div className="switch-row">
            <label htmlFor={ids.reinvest} className="switch-row__label">
              Retirar os proventos todo mês{" "}
              <span className="field__hint">
                {keepIncome
                  ? "Os proventos saem da carteira: o valor anda só pelo preço e a renda só cresce com o aporte."
                  : "O padrão: os proventos são reinvestidos e compram mais renda."}
              </span>
            </label>
            <input
              id={ids.reinvest}
              type="checkbox"
              role="switch"
              className="switch"
              checked={keepIncome}
              onChange={(event) => setKeepIncome(event.target.checked)}
            />
          </div>
        </fieldset>

        <fieldset className="objective-form__premise">
          <legend className="field__label">Taxas (em branco = o padrão)</legend>
          <div className="objective-form__grid">
            {shown.map((key) => {
              const id = `${ids.rate}-${key}`;
              const check = checks[key];
              return (
                <div key={key} className="field">
                  <label htmlFor={id} className="field__label">
                    {RATES[key].label}
                  </label>
                  <input
                    id={id}
                    className="text-input"
                    inputMode="decimal"
                    placeholder="padrão"
                    value={rates[key]}
                    aria-invalid={check?.error ? true : undefined}
                    aria-describedby={`${id}-hint`}
                    onChange={(event) => setRates((current) => ({ ...current, [key]: event.target.value }))}
                  />
                  <span id={`${id}-hint`} className={check?.error ? "rule-row__error" : "field__hint"} role={check?.error ? "alert" : undefined}>
                    {check?.error ?? `Padrão: ${defaultHint[key]}.`}
                  </span>
                </div>
              );
            })}
          </div>
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
