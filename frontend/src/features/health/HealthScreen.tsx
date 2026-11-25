import { motion } from "motion/react";
import { useEffect, useId, useMemo, useState } from "react";
import { errorMessage } from "../../api/client";
import type { Health, HealthRuleResult, HealthWarning, RuleChange, RuleId, RulesRequest } from "../../api/healthTypes";
import { GlassCard } from "../../components/GlassCard";
import { IconAlert, IconCheck } from "../../components/icons";
import { LimitBar } from "../../components/LimitBar";
import { MagneticButton } from "../../components/MagneticButton";
import { Plumb } from "../../components/Plumb";
import { Skeleton } from "../../components/Skeleton";
import { useToast } from "../../components/toastContext";
import { CLASS_META } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { useMoney } from "../../lib/privacy";
import { formatRelative, parseDecimal, plural } from "../../lib/format";
import {
  formatRuleValue,
  limitText,
  RULE_TEXT,
  ruleName,
  STATUS_TEXT,
  thresholdError,
  warningAction,
  warningMembers,
  warningTitle,
} from "../../lib/healthText";
import { useHealth, useSaveRules } from "./useHealth";

export function HealthScreen() {
  const health = useHealth();

  return (
    <div className="screen health">
      <header className="screen__header">
        <p className="eyebrow">Saúde</p>
        <h1 className="screen__title">Saúde da carteira</h1>
        <p className="screen__lead">
          Avisos sobre o que a meta por classe não enxerga: concentração num ativo ou num setor e classe rasa ou vazia.
          Não há nota: cada regra é sua, você liga, desliga e escolhe o limite.
        </p>
      </header>

      {health.isPending && (
        <GlassCard as="section" aria-label="Carregando a saúde da carteira" className="health__loading">
          <Skeleton height={28} width="55%" />
          <Skeleton height={64} />
        </GlassCard>
      )}
      {health.isError && (
        <GlassCard as="section" role="alert" className="health__error">
          <p>{errorMessage(health.error)}</p>
          <MagneticButton variant="ghost" magnetic={false} onClick={() => void health.refetch()}>
            Tentar de novo
          </MagneticButton>
        </GlassCard>
      )}
      {health.data && (
        <>
          <Verdict health={health.data} />
          {health.data.warnings.length > 0 && (
            <ul className="health__warnings" aria-label="Avisos">
              {health.data.warnings.map((warning, index) => (
                <WarningCard key={`${warning.rule}-${index}`} warning={warning} index={index} />
              ))}
            </ul>
          )}
          <RulesCard rules={health.data.rules} />
          <SourcesCard health={health.data} />
        </>
      )}
    </div>
  );
}

function Verdict({ health }: { health: Health }) {
  const { warnings, rules } = health;
  const measured = rules.filter((r) => r.status === "ok" || r.status === "warning");
  const calm = warnings.length === 0 && measured.length > 0;
  const over = measured.filter((r) => r.status === "warning").length;

  return (
    <GlassCard as="section" className="health__verdict" aria-label="Resumo">
      <Plumb state={calm ? "settled" : "idle"} size={64} label={calm ? "Prumo no eixo" : "Prumo"} />
      <div role="status">
        {warnings.length > 0 ? (
          <>
            <p className="health__count">
              {warnings.length} {plural(warnings.length, ["aviso", "avisos"])}
            </p>
            <p className="field__hint">
              Das {measured.length} {plural(measured.length, ["regra que tinha", "regras que tinham"])} o que medir,{" "}
              {over} {plural(over, ["passou", "passaram"])} do limite.
            </p>
          </>
        ) : calm ? (
          <>
            <p className="health__count">Nenhum aviso</p>
            <p className="field__hint">
              A carteira está dentro dos seus limites em {measured.length} {plural(measured.length, ["regra", "regras"])}.
            </p>
          </>
        ) : (
          <>
            <p className="health__count">Ainda sem o que medir</p>
            <p className="field__hint">
              Cadastre ativos e defina as metas em Metas para as regras terem o que olhar. Ligadas e sem dados aparecem
              abaixo como “{STATUS_TEXT.not_applicable}”.
            </p>
          </>
        )}
      </div>
    </GlassCard>
  );
}

function WarningCard({ warning, index }: { warning: HealthWarning; index: number }) {
  const money = useMoney();
  const action = warningAction(warning, money);
  const members = warningMembers(warning);
  const percent = warning.unit === "percent";
  const title = warningTitle(warning);

  return (
    <motion.li
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: index * 0.06, type: "spring", stiffness: 260, damping: 26 }}
    >
      <GlassCard as="article" className="warning-card" aria-label={title}>
        <header className="warning-card__head">
          <IconAlert className="warning-card__icon" />
          <h2 className="warning-card__title">{title}</h2>
        </header>
        {members && <p className="field__hint">{members}</p>}
        <dl className="warning-card__numbers">
          <div>
            <dt>Agora</dt>
            <dd>{formatRuleValue(warning.unit, warning.current)}</dd>
          </div>
          <div>
            <dt>{warning.rule === "min_assets" || warning.rule === "empty_class" ? "Mínimo" : "Limite"}</dt>
            <dd>{formatRuleValue(warning.unit, warning.limit)}</dd>
          </div>
        </dl>
        {percent && (
          <LimitBar
            current={warning.current}
            limit={warning.limit}
            label={`${title}; ${limitText(warning).toLowerCase()}`}
          />
        )}
        <p className="warning-card__action">
          <span className="warning-card__what">O que fazer.</span> {action.text}
        </p>
        {action.link && (
          <a className="link-btn" href={action.link.href}>
            {action.link.label}
          </a>
        )}
      </GlassCard>
    </motion.li>
  );
}

type Draft = Record<RuleId, { enabled: boolean; threshold: string }>;

function toDraft(rules: HealthRuleResult[]): Draft {
  return Object.fromEntries(
    rules.map((r) => [r.rule, { enabled: r.enabled, threshold: r.threshold === null ? "" : String(r.threshold).replace(".", ",") }]),
  ) as Draft;
}

function RulesCard({ rules }: { rules: HealthRuleResult[] }) {
  const saved = useMemo(() => toDraft(rules), [rules]);
  const [draft, setDraft] = useState<Draft>(saved);
  const [touched, setTouched] = useState(false);
  const save = useSaveRules();
  const toast = useToast();
  const baseId = useId();

  // Enquanto a pessoa não mexeu, a tela acompanha o que o servidor diz.
  useEffect(() => {
    if (!touched) setDraft(saved);
  }, [saved, touched]);

  const errors = useMemo(() => {
    const found: Partial<Record<RuleId, string>> = {};
    for (const rule of rules) {
      if (rule.unit === null || !draft[rule.rule].enabled) continue;
      const message = thresholdError(rule.unit, parseDecimal(draft[rule.rule].threshold));
      if (message) found[rule.rule] = message;
    }
    return found;
  }, [rules, draft]);

  const changes = useMemo(() => {
    const body: RulesRequest["rules"] = {};
    for (const rule of rules) {
      const now = draft[rule.rule];
      const before = saved[rule.rule];
      const change: RuleChange = {};
      if (now.enabled !== before.enabled) change.enabled = now.enabled;
      const value = parseDecimal(now.threshold);
      if (rule.unit !== null && now.threshold !== before.threshold && value !== null) change.threshold = value;
      if (Object.keys(change).length > 0) body[rule.rule] = change;
    }
    return body;
  }, [rules, draft, saved]);

  const dirty = Object.keys(changes).length > 0;
  const invalid = Object.keys(errors).length > 0;

  const edit = (rule: RuleId, patch: Partial<Draft[RuleId]>) => {
    setTouched(true);
    setDraft((current) => ({ ...current, [rule]: { ...current[rule], ...patch } }));
  };

  const submit = () => {
    save.mutate(
      { rules: changes },
      {
        onSuccess: () => {
          setTouched(false);
          toast({ tone: "success", message: "Regras salvas." });
        },
        onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
      },
    );
  };

  return (
    <GlassCard as="section" className="health__rules" aria-labelledby={`${baseId}-title`}>
      <h2 id={`${baseId}-title`} className="section-title">
        Suas regras
      </h2>
      <ul className="rule-list">
        {rules.map((rule) => {
          const text = RULE_TEXT[rule.rule];
          const id = `${baseId}-${rule.rule}`;
          const state = draft[rule.rule];
          const error = errors[rule.rule];
          const status = rule.status;
          return (
            <li key={rule.rule} className="rule-row">
              <div className="rule-row__main">
                <div className="rule-row__sentence">
                  <span className="rule-row__lead">{text.lead}</span>
                  {rule.unit !== null && (
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
                  {text.tail && <span className="rule-row__tail">{text.tail}</span>}
                </div>
                <p className="field__hint">{text.why}</p>
                {error && state.enabled && (
                  <p id={`${id}-error`} className="rule-row__error" role="alert">
                    {error}
                  </p>
                )}
                <div className="rule-row__meta">
                  <span className={cx("rule-status", `rule-status--${status}`)}>
                    {status === "ok" && <IconCheck width={16} height={16} />}
                    {status === "warning" && <IconAlert width={16} height={16} />}
                    {STATUS_TEXT[status]}
                  </span>
                  {rule.default_threshold !== null && state.threshold !== String(rule.default_threshold).replace(".", ",") && (
                    <button
                      type="button"
                      className="link-btn"
                      onClick={() => edit(rule.rule, { threshold: String(rule.default_threshold).replace(".", ",") })}
                    >
                      Voltar para {formatRuleValue(rule.unit ?? "percent", rule.default_threshold)}
                    </button>
                  )}
                </div>
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
        })}
      </ul>
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
    </GlassCard>
  );
}

function SourcesCard({ health }: { health: Health }) {
  const titleId = useId();
  const origin: Record<string, string> = {
    b3_stocks: "Classificação Setorial da B3",
    cvm_reits: "informe mensal de FII da CVM (tipo pelo balanço do fundo)",
  };

  return (
    <GlassCard as="section" className="health__sources" aria-labelledby={titleId}>
      <h2 id={titleId} className="section-title">
        De onde vem o setor
      </h2>
      <ul className="source-list">
        {health.sources.map((source) => (
          <li key={source.source}>
            <strong>{CLASS_META[source.asset_class].label}</strong>: {origin[source.source]}.{" "}
            {source.updated_at ? `Atualizado ${formatRelative(source.updated_at)}.` : "Ainda não buscado."}
            {source.failing && (
              <span className="source-list__warn">
                {" "}
                A última busca falhou{source.updated_at ? "; vale a anterior" : ""}. O Prumo tenta de novo sozinho.
              </span>
            )}
          </li>
        ))}
        <li>
          <strong>Ações e REITs dos EUA, cripto e renda fixa</strong>: sem setor. A fonte pública dos EUA (SEC) exige
          identificar quem consulta com nome e e-mail, e o Prumo não envia dado de ninguém.
        </li>
      </ul>
      {health.unclassified_count > 0 && (
        <p className="field__hint">
          {health.unclassified_count} {plural(health.unclassified_count, ["ativo", "ativos"])} em Ações Nacionais ou FIIs
          sem setor na fonte (comum em papel novo ou fundo sem informe). {plural(health.unclassified_count, ["Ele", "Eles"])}{" "}
          não entra{health.unclassified_count === 1 ? "" : "m"} na regra de setor.
        </p>
      )}
    </GlassCard>
  );
}
