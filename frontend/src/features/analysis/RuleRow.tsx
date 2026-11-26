import type { RuleResult } from "../../api/analysisTypes";
import { IconAlert, IconCheck } from "../../components/icons";
import { LimitBar } from "../../components/LimitBar";
import {
  limitLabel,
  NO_DATA_TEXT,
  ruleAction,
  ruleExplanation,
  ruleName,
  ruleSentence,
  STATUS_TEXT,
} from "../../lib/analysisText";
import { cx } from "../../lib/cx";
import { formatRuleValue } from "../../lib/healthText";
import { useMoney } from "../../lib/privacy";
import { Finding } from "./Finding";

/**
 * Uma regra no painel: o estado em texto (a cor só reforça), o número de agora
 * e o limite, a conta por trás e, num ponto a ver, o que fazer. Sem dado é
 * dito como sem dado, nunca como "tudo certo" nem como falha.
 */
export function RuleRow({ result }: { result: RuleResult }) {
  const money = useMoney();
  const { status } = result;
  const findings = result.detail?.kind === "findings" ? result.detail.warnings : [];
  const explanation = findings.length > 0 ? null : ruleExplanation(result, money);
  const action = status === "warning" && findings.length === 0 ? ruleAction(result, money) : null;
  const measured = status === "ok" || status === "warning";
  const second = limitLabel(result);
  const showLimitBar =
    status !== "off" &&
    result.unit === "percent" &&
    result.direction === "at_most" &&
    result.current !== null &&
    result.limit !== null &&
    findings.length === 0;

  return (
    <li className={cx("analysis-rule", `analysis-rule--${status}`)}>
      <div className="analysis-rule__head">
        <span className={cx("rule-status", `rule-status--${status}`)}>
          {status === "ok" && <IconCheck width={16} height={16} />}
          {status === "warning" && <IconAlert width={16} height={16} />}
          {STATUS_TEXT[status]}
        </span>
        <h3 className="analysis-rule__name">{ruleSentence(result)}</h3>
      </div>

      {measured && result.current !== null && findings.length === 0 && (
        <dl className="analysis-rule__numbers">
          <div>
            <dt>Agora</dt>
            <dd>{formatRuleValue(result.unit, result.current)}</dd>
          </div>
          {second !== null && result.limit !== null && (
            <div>
              <dt>{second}</dt>
              <dd>{formatRuleValue(result.unit, result.limit)}</dd>
            </div>
          )}
        </dl>
      )}
      {showLimitBar && result.current !== null && result.limit !== null && (
        <LimitBar
          current={result.current}
          limit={result.limit}
          label={`${ruleName(result.rule)}: ${formatRuleValue("percent", result.current)}, limite ${formatRuleValue("percent", result.limit)}`}
        />
      )}
      {explanation && <p className="analysis-rule__text">{explanation}</p>}
      {action && (
        <>
          <p className="warning-card__action">
            <span className="warning-card__what">O que fazer.</span> {action.text}
          </p>
          {action.link && (
            <a className="link-btn" href={action.link.href}>
              {action.link.label}
            </a>
          )}
        </>
      )}
      {findings.length > 0 && (
        <div className="analysis-rule__findings">
          {findings.map((warning, index) => (
            <Finding key={`${warning.rule}-${index}`} warning={warning} />
          ))}
        </div>
      )}
      {status === "no_data" && <p className="field__hint">{NO_DATA_TEXT[result.rule]}</p>}
      {status === "off" && (
        <p className="field__hint">Desligada: não entra na pontuação. Religue em Ajustes, Análise.</p>
      )}
    </li>
  );
}
