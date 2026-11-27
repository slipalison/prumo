import { useEffect, useId, useMemo, useState, type ReactNode } from "react";
import { errorMessage } from "../../api/client";
import type { AnalysisConfig, AnalysisRuleId, ConfigRequest, DimensionId, RuleChange, RuleConfig } from "../../api/analysisTypes";
import { GlassCard } from "../../components/GlassCard";
import { InfoTip } from "../../components/InfoTip";
import { Grid, Page, PageHeader, Panel } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { useToast } from "../../components/toastContext";
import { COUNT_MAX, DIMENSION_TEXT, RULE_TEXT, ruleName } from "../../lib/analysisText";
import { brickTargetMath, cdiText, paperTargetMath, realRateText } from "../../lib/ceilingText";
import { parseDecimal } from "../../lib/format";
import { formatRuleValue, thresholdError } from "../../lib/healthText";
import { useAnalysisConfig, useSaveAnalysisConfig } from "./useAnalysis";

/**
 * As dimensões em duas colunas, na ordem da tela (uma coluna só no celular):
 * as duas pilhas ficam com altura parecida (10 + 6 + 1 regras e 9 + 8).
 */
const COLUMNS: [DimensionId[], DimensionId[]] = [
  ["diversification", "dividends"],
  ["returns", "quality"],
];

type Draft = {
  rules: Record<AnalysisRuleId, { enabled: boolean; threshold: string }>;
  bazinYield: string;
  brickPremium: string;
  paperPremium: string;
};

const text = (value: number | null): string => (value === null ? "" : String(value).replace(".", ","));

function toDraft(config: AnalysisConfig): Draft {
  return {
    rules: Object.fromEntries(config.rules.map((r) => [r.rule, { enabled: r.enabled, threshold: text(r.threshold) }])) as Draft["rules"],
    bazinYield: text(config.bazin_yield),
    brickPremium: text(config.market.brick_premium),
    paperPremium: text(config.market.paper_premium),
  };
}

/** O rendimento-alvo do Bazin: percentual maior que 0 e até 100, com até duas casas. */
const bazinError = (value: string) => thresholdError("percent", parseDecimal(value));

/** O prêmio de um tipo de FII: de 0 a 20 pontos percentuais, com até duas casas. */
function premiumError(value: string): string | null {
  const number = parseDecimal(value);
  if (number === null || number < 0 || number > 20) return "Use de 0 a 20 pontos percentuais.";
  if (Math.round(number * 100) !== number * 100) return "Use até duas casas decimais.";
  return null;
}

type PremiumKey = "brickPremium" | "paperPremium";

/**
 * Ajustes, Análise: liga, desliga e escolhe o limite de cada regra da Análise
 * (e o rendimento-alvo da referência de Bazin). O que era a "Saúde da
 * carteira" é a Análise; o endereço antigo (`#/ajustes/saude`) leva para cá.
 */
export function AnalysisSettings() {
  const config = useAnalysisConfig();

  return (
    <Page className="analysis-settings">
      <PageHeader
        title="Regras da análise"
        infoLabel="Sobre as regras da análise"
        info={
          <>
            Cada regra é uma pergunta com um limite que é seu: você liga, desliga e escolhe o número. A pontuação de uma
            dimensão é só o percentual das regras ligadas, com dado, que estão ok.
          </>
        }
        actions={
          <a className="link-btn" href="#/analise">
            Ver a análise
          </a>
        }
      />

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
    </Page>
  );
}

function RulesCard({ config }: Readonly<{ config: AnalysisConfig }>) {
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
    const found: Partial<Record<AnalysisRuleId | "bazin" | PremiumKey, string>> = {};
    for (const rule of config.rules) {
      if (rule.default_threshold === null || !draft.rules[rule.rule].enabled) continue;
      const message = thresholdError(rule.unit, parseDecimal(draft.rules[rule.rule].threshold), COUNT_MAX[rule.rule]);
      if (message) found[rule.rule] = message;
    }
    const bazin = bazinError(draft.bazinYield);
    if (bazin) found.bazin = bazin;
    for (const key of ["brickPremium", "paperPremium"] as const) {
      const message = premiumError(draft[key]);
      if (message) found[key] = message;
    }
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
    const brick = parseDecimal(draft.brickPremium);
    if (draft.brickPremium !== saved.brickPremium && brick !== null) body.brick_premium = brick;
    const paper = parseDecimal(draft.paperPremium);
    if (draft.paperPremium !== saved.paperPremium && paper !== null) body.paper_premium = paper;
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
            <InfoTip label={`Por que: ${ruleName(rule.rule)}`}>{copy.why}</InfoTip>
          </div>
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

  type ValueRow = {
    id: string;
    label: string;
    unit: string;
    value: string;
    error: string | undefined;
    onChange: (value: string) => void;
    /** O padrão de fábrica, para o "Voltar para". */
    defaultValue?: number;
    tip?: ReactNode;
  };

  /** Uma linha com campo numérico (o rendimento-alvo, um prêmio). */
  const valueRow = ({ id, label, unit, value, error, onChange, defaultValue, tip }: ValueRow) => (
    <li className="rule-row">
      <div className="rule-row__main">
        <div className="rule-row__sentence">
          <label className="rule-row__lead" htmlFor={id}>
            {label}
          </label>
          <span className="rule-row__field">
            <input
              id={id}
              className="text-input rule-row__input"
              inputMode="decimal"
              value={value}
              aria-invalid={error ? true : undefined}
              aria-describedby={error ? `${id}-error` : undefined}
              onChange={(event) => {
                setTouched(true);
                onChange(event.target.value);
              }}
            />
            <span className="rule-row__unit">{unit}</span>
          </span>
          {tip && <InfoTip label={`Sobre: ${label}`}>{tip}</InfoTip>}
        </div>
        {error && (
          <p id={`${id}-error`} className="rule-row__error" role="alert">
            {error}
          </p>
        )}
        {defaultValue !== undefined && value !== text(defaultValue) && (
          <div className="rule-row__meta">
            <button
              type="button"
              className="link-btn"
              onClick={() => {
                setTouched(true);
                onChange(text(defaultValue));
              }}
            >
              Voltar para {text(defaultValue)} p.p.
            </button>
          </div>
        )}
      </div>
    </li>
  );

  const panel = (group: DimensionId) => (
    <Panel key={group} className="rules-panel" title={DIMENSION_TEXT[group].title} info={DIMENSION_TEXT[group].lead}>
      <ul className="rule-list">{config.rules.filter((r) => r.dimension === group).map(row)}</ul>
    </Panel>
  );

  return (
    <>
      <Grid cols={2} alignStart>
        <div className="rules-column">
          {COLUMNS[0].map(panel)}
          <Panel
            className="rules-panel"
            title="Referências de preço"
            infoLabel="Sobre o rendimento-alvo e os prêmios"
            info="O Bazin é a média anual de dividendos por cota dividida pelo rendimento-alvo. O Rendimento dos FIIs divide os proventos de 12 meses pelo alvo do tipo: juro real do Tesouro mais o prêmio do tijolo, ou CDI líquido mais o prêmio do papel. São referências públicas, não recomendação."
          >
            <ul className="rule-list">
              {valueRow({
                id: `${baseId}-bazin-yield`,
                label: "Rendimento-alvo do Bazin",
                unit: "% ao ano",
                value: draft.bazinYield,
                error: errors.bazin,
                onChange: (value) => setDraft((current) => ({ ...current, bazinYield: value })),
              })}
              {valueRow({
                id: `${baseId}-brick-premium`,
                label: "Prêmio do FII de tijolo",
                unit: "p.p. sobre o juro real",
                value: draft.brickPremium,
                error: errors.brickPremium,
                onChange: (value) => setDraft((current) => ({ ...current, brickPremium: value })),
                defaultValue: config.market.default_brick_premium,
                tip: (
                  <>
                    Sobre o {realRateText(config.market)}.{" "}
                    {brickTargetMath(config.market) ? `Alvo de hoje: ${brickTargetMath(config.market)}.` : ""} O prêmio paga o
                    risco do imóvel (vacância, inadimplência) acima do título público.
                  </>
                ),
              })}
              {valueRow({
                id: `${baseId}-paper-premium`,
                label: "Prêmio do FII de papel",
                unit: "p.p. sobre o CDI líquido",
                value: draft.paperPremium,
                error: errors.paperPremium,
                onChange: (value) => setDraft((current) => ({ ...current, paperPremium: value })),
                defaultValue: config.market.default_paper_premium,
                tip: (
                  <>
                    Sobre o {cdiText(config.market)}.{" "}
                    {paperTargetMath(config.market) ? `Alvo de hoje: ${paperTargetMath(config.market)}.` : ""} O prêmio paga o
                    risco de crédito dos recebíveis acima do CDI.
                  </>
                ),
              })}
            </ul>
            <p className="rules-market" aria-live="polite">
              <span>Hoje: {realRateText(config.market)}.</span> <span>{cdiText(config.market)}.</span>
            </p>
          </Panel>
        </div>
        <div className="rules-column">{COLUMNS[1].map(panel)}</div>
      </Grid>

      <div className={dirty ? "rules-actions rules-actions--dirty" : "rules-actions"}>
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
