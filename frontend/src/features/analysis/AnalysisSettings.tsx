import { useEffect, useId, useMemo, useState } from "react";
import { errorMessage } from "../../api/client";
import type { AnalysisConfig, AnalysisRuleId, ConfigRequest, DimensionId, RuleChange, RuleConfig } from "../../api/analysisTypes";
import { GlassCard } from "../../components/GlassCard";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { useToast } from "../../components/toastContext";
import { COUNT_MAX, DIMENSION_TEXT, RULE_TEXT, ruleName } from "../../lib/analysisText";
import { parseDecimal } from "../../lib/format";
import { formatRuleValue, thresholdError } from "../../lib/healthText";
import { useAnalysisConfig, useSaveAnalysisConfig } from "./useAnalysis";

/** As dimensões que já têm regra, na ordem da tela. */
const GROUPS: DimensionId[] = ["diversification", "dividends", "returns"];

type Draft = {
  rules: Record<AnalysisRuleId, { enabled: boolean; threshold: string }>;
  bazinYield: string;
};

const text = (value: number | null): string => (value === null ? "" : String(value).replace(".", ","));

function toDraft(config: AnalysisConfig): Draft {
  return {
    rules: Object.fromEntries(config.rules.map((r) => [r.rule, { enabled: r.enabled, threshold: text(r.threshold) }])) as Draft["rules"],
    bazinYield: text(config.bazin_yield),
  };
}

/** O rendimento-alvo do Bazin: percentual maior que 0 e até 100, com até duas casas. */
const bazinError = (value: string) => thresholdError("percent", parseDecimal(value));

/**
 * Ajustes, Análise: liga, desliga e escolhe o limite de cada regra da Análise
 * (e o rendimento-alvo da referência de Bazin). O que era a "Saúde da
 * carteira" é a Análise; o endereço antigo (`#/ajustes/saude`) leva para cá.
 */
export function AnalysisSettings() {
  const config = useAnalysisConfig();

  return (
    <div className="screen analysis-settings">
      <header className="screen__header">
        <p className="eyebrow">Análise</p>
        <h1 className="screen__title">Regras da análise</h1>
        <p className="screen__lead">
          Cada regra é uma pergunta com um limite que é seu: você liga, desliga e escolhe o número. A pontuação de uma
          dimensão é só o percentual das regras ligadas, com dado, que estão ok. O resultado está em{" "}
          <a className="link-btn" href="#/analise">
            Análise
          </a>
          .
        </p>
      </header>

      {config.isPending && (
        <GlassCard as="section" aria-label="Carregando as regras" className="analysis__loading">
          <Skeleton height={28} width="55%" />
          <Skeleton height={64} />
        </GlassCard>
      )}
      {config.isError && (
        <GlassCard as="section" role="alert" className="analysis__error">
          <p>{errorMessage(config.error)}</p>
          <MagneticButton variant="ghost" magnetic={false} onClick={() => void config.refetch()}>
            Tentar de novo
          </MagneticButton>
        </GlassCard>
      )}
      {config.data && <RulesCard config={config.data} />}
    </div>
  );
}

function RulesCard({ config }: { config: AnalysisConfig }) {
  const saved = useMemo(() => toDraft(config), [config]);
  const [draft, setDraft] = useState<Draft>(saved);
  const [touched, setTouched] = useState(false);
  const save = useSaveAnalysisConfig();
  const toast = useToast();
  const baseId = useId();

  // Enquanto a pessoa não mexeu, a tela acompanha o que o servidor diz.
  useEffect(() => {
    if (!touched) setDraft(saved);
  }, [saved, touched]);

  const errors = useMemo(() => {
    const found: Partial<Record<AnalysisRuleId | "bazin", string>> = {};
    for (const rule of config.rules) {
      if (rule.default_threshold === null || !draft.rules[rule.rule].enabled) continue;
      const message = thresholdError(rule.unit, parseDecimal(draft.rules[rule.rule].threshold), COUNT_MAX[rule.rule]);
      if (message) found[rule.rule] = message;
    }
    const bazin = bazinError(draft.bazinYield);
    if (bazin) found.bazin = bazin;
    return found;
  }, [config, draft]);

  const changes = useMemo(() => {
    const body: ConfigRequest = {};
    const rules: NonNullable<ConfigRequest["rules"]> = {};
    for (const rule of config.rules) {
      const now = draft.rules[rule.rule];
      const before = saved.rules[rule.rule];
      const change: RuleChange = {};
      if (now.enabled !== before.enabled) change.enabled = now.enabled;
      const value = parseDecimal(now.threshold);
      if (rule.default_threshold !== null && now.threshold !== before.threshold && value !== null) change.threshold = value;
      if (Object.keys(change).length > 0) rules[rule.rule] = change;
    }
    if (Object.keys(rules).length > 0) body.rules = rules;
    const bazin = parseDecimal(draft.bazinYield);
    if (draft.bazinYield !== saved.bazinYield && bazin !== null) body.bazin_yield = bazin;
    return body;
  }, [config, draft, saved]);

  const dirty = Object.keys(changes).length > 0;
  const invalid = Object.keys(errors).length > 0;

  const edit = (rule: AnalysisRuleId, patch: Partial<Draft["rules"][AnalysisRuleId]>) => {
    setTouched(true);
    setDraft((current) => ({ ...current, rules: { ...current.rules, [rule]: { ...current.rules[rule], ...patch } } }));
  };

  const submit = () => {
    save.mutate(changes, {
      onSuccess: () => {
        setTouched(false);
        toast({ tone: "success", message: "Regras salvas." });
      },
      onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
    });
  };

  const row = (rule: RuleConfig) => {
    const copy = RULE_TEXT[rule.rule];
    const id = `${baseId}-${rule.rule}`;
    const state = draft.rules[rule.rule];
    const error = errors[rule.rule];
    const editable = rule.default_threshold !== null;
    const defaultText = text(rule.default_threshold);

    return (
      <li key={rule.rule} className="rule-row">
        <div className="rule-row__main">
          <div className="rule-row__sentence">
            <span className="rule-row__lead">{copy.lead}</span>
            {editable && (
              <span className="rule-row__field">
                <input
                  id={`${id}-limit`}
                  className="text-input rule-row__input"
                  inputMode="decimal"
                  value={state.threshold}
                  aria-label={`Limite: ${ruleName(rule.rule)}`}
                  aria-invalid={error ? true : undefined}
                  aria-describedby={error ? `${id}-error` : undefined}
                  disabled={!state.enabled}
                  onChange={(event) => edit(rule.rule, { threshold: event.target.value })}
                />
                {rule.unit === "percent" && <span className="rule-row__unit">%</span>}
              </span>
            )}
            {copy.tail && <span className="rule-row__tail">{copy.tail}</span>}
          </div>
          <p className="field__hint">{copy.why}</p>
          {error && state.enabled && (
            <p id={`${id}-error`} className="rule-row__error" role="alert">
              {error}
            </p>
          )}
          {editable && state.threshold !== defaultText && rule.default_threshold !== null && (
            <div className="rule-row__meta">
              <button type="button" className="link-btn" onClick={() => edit(rule.rule, { threshold: defaultText })}>
                Voltar para {formatRuleValue(rule.unit, rule.default_threshold)}
              </button>
            </div>
          )}
        </div>
        <input
          id={`${id}-switch`}
          type="checkbox"
          role="switch"
          className="switch"
          aria-label={ruleName(rule.rule)}
          checked={state.enabled}
          onChange={(event) => edit(rule.rule, { enabled: event.target.checked })}
        />
      </li>
    );
  };

  return (
    <>
      {GROUPS.map((group) => (
        <GlassCard key={group} as="section" className="health__rules" aria-labelledby={`${baseId}-${group}`}>
          <h2 id={`${baseId}-${group}`} className="section-title">
            {DIMENSION_TEXT[group].title}
          </h2>
          <ul className="rule-list">{config.rules.filter((r) => r.dimension === group).map(row)}</ul>
        </GlassCard>
      ))}

      <GlassCard as="section" className="health__rules" aria-labelledby={`${baseId}-quality`}>
        <h2 id={`${baseId}-quality`} className="section-title">
          {DIMENSION_TEXT.quality.title}
        </h2>
        <p className="field__hint">As regras desta dimensão chegam junto com os dados de fundamentos da CVM.</p>
      </GlassCard>

      <GlassCard as="section" className="health__rules" aria-labelledby={`${baseId}-bazin`}>
        <h2 id={`${baseId}-bazin`} className="section-title">
          Referências de preço
        </h2>
        <div className="rule-row">
          <div className="rule-row__main">
            <div className="rule-row__sentence">
              <label className="rule-row__lead" htmlFor={`${baseId}-bazin-yield`}>
                Rendimento-alvo do Bazin
              </label>
              <span className="rule-row__field">
                <input
                  id={`${baseId}-bazin-yield`}
                  className="text-input rule-row__input"
                  inputMode="decimal"
                  value={draft.bazinYield}
                  aria-invalid={errors.bazin ? true : undefined}
                  aria-describedby={errors.bazin ? `${baseId}-bazin-error` : undefined}
                  onChange={(event) => {
                    setTouched(true);
                    setDraft((current) => ({ ...current, bazinYield: event.target.value }));
                  }}
                />
                <span className="rule-row__unit">% ao ano</span>
              </span>
            </div>
            <p className="field__hint">
              O preço de referência de Bazin é a média anual de dividendos por cota dividida por este rendimento. É uma
              referência pública, não uma recomendação.
            </p>
            {errors.bazin && (
              <p id={`${baseId}-bazin-error`} className="rule-row__error" role="alert">
                {errors.bazin}
              </p>
            )}
          </div>
        </div>
      </GlassCard>

      <div className="health__actions">
        <MagneticButton disabled={!dirty || invalid} loading={save.isPending} onClick={submit}>
          Salvar regras
        </MagneticButton>
        {dirty && (
          <button
            type="button"
            className="link-btn"
            onClick={() => {
              setTouched(false);
              setDraft(saved);
            }}
          >
            Descartar mudanças
          </button>
        )}
      </div>
    </>
  );
}
