import type { RuleResult, RuleStatus } from "../../api/analysisTypes";
import type { HealthWarning } from "../../api/healthTypes";
import { AssetIcon } from "../../components/AssetIcon";
import { DataTable, StatusPill, type DataColumn, type Status } from "../../components/DataTable";
import { InfoTip } from "../../components/InfoTip";
import { LimitBar } from "../../components/LimitBar";
import {
  limitCell,
  NO_DATA_TEXT,
  ruleAction,
  ruleExplanation,
  ruleName,
  ruleSentence,
  STATUS_TEXT,
} from "../../lib/analysisText";
import { iconSubjectOfLabel } from "../../lib/assetIcon";
import { formatRuleValue, warningAction, warningAssets, warningMembers, warningTitle, type WarningAction } from "../../lib/healthText";
import { useMoney } from "../../lib/privacy";
import type { Money } from "../../lib/money";

const PILL: Record<RuleStatus, Status> = { warning: "attention", ok: "ok", no_data: "nodata", off: "off" };

const findingsOf = (result: RuleResult): HealthWarning[] =>
  result.detail?.kind === "findings" ? result.detail.warnings : [];

/**
 * A chave de um achado: quem ele aponta (o ativo, o setor, a classe), que não
 * se repete dentro de uma regra e não muda entre uma renderização e outra.
 */
function findingKey(warning: HealthWarning): string {
  const { subject } = warning;
  switch (subject.kind) {
    case "asset":
      return `${warning.rule}-asset-${subject.asset_id}`;
    case "top_assets":
      return `${warning.rule}-top-${subject.labels.join(",")}`;
    case "sector":
      return `${warning.rule}-sector-${subject.asset_class}-${subject.sector}`;
    case "class":
      return `${warning.rule}-class-${subject.asset_class}`;
    case "income_months":
      return `${warning.rule}-income-months`;
    case "payer":
      return `${warning.rule}-payer-${subject.asset_id ?? subject.label}`;
  }
}

/** O número de agora: só quando a regra mediu, e não nas regras sem limite que contam achados. */
function nowValue(result: RuleResult): string | null {
  const measured = result.status === "ok" || result.status === "warning";
  if (!measured || result.current === null) return null;
  if (findingsOf(result).length > 0 && result.direction === null) return null;
  return formatRuleValue(result.unit, result.current);
}

/** A barra fina do limite, nas regras de "no máximo X%". */
function showsBar(result: RuleResult): result is RuleResult & { current: number; limit: number } {
  return (
    result.status !== "off" &&
    result.unit === "percent" &&
    result.direction === "at_most" &&
    result.current !== null &&
    result.limit !== null
  );
}

/** Um achado de uma das sete regras originais da Saúde, em uma linha: quem passou e o número. */
function FindingLine({ warning }: Readonly<{ warning: HealthWarning }>) {
  const assets = warningAssets(warning);
  const members = warningMembers(warning);
  return (
    <li className="rule-finding">
      {assets.length > 0 && (
        <span className="rule-finding__icons" aria-hidden="true">
          {assets.slice(0, 5).map((asset) => (
            <AssetIcon key={asset.label} size="sm" assetClass={asset.assetClass} {...iconSubjectOfLabel(asset.label)} />
          ))}
        </span>
      )}
      <span className="rule-finding__text">
        {warningTitle(warning)}
        {members && <span className="rule-finding__members">{members}</span>}
      </span>
    </li>
  );
}

/** A conta por trás do estado, em uma linha (o diagnóstico). */
function Diagnosis({ result, money }: Readonly<{ result: RuleResult; money: Money }>) {
  const findings = findingsOf(result);
  if (findings.length > 0) {
    return (
      <ul className="rule-findings">
        {findings.map((warning) => (
          <FindingLine key={findingKey(warning)} warning={warning} />
        ))}
      </ul>
    );
  }
  if (result.status === "no_data") return <span className="rule-diagnosis">{NO_DATA_TEXT[result.rule]}</span>;
  if (result.status === "off") {
    return <span className="rule-diagnosis">Desligada: não entra na pontuação. Religue em Ajustes, Análise.</span>;
  }
  const explanation = ruleExplanation(result, money);
  return explanation ? <span className="rule-diagnosis">{explanation}</span> : null;
}

/** O que fazer num ponto a ver: o caminho à vista, o texto inteiro (com o valor do aporte) no ⓘ. */
function Action({ result, money }: Readonly<{ result: RuleResult; money: Money }>) {
  const findings = findingsOf(result);
  const actions: Array<{ key: string; title?: string; action: WarningAction }> =
    findings.length > 0
      ? findings.map((warning) => ({
          key: findingKey(warning),
          title: findings.length > 1 ? warningTitle(warning) : undefined,
          action: warningAction(warning, money),
        }))
      : [ruleAction(result, money)].filter((a): a is WarningAction => a !== null).map((action) => ({ key: result.rule, action }));
  const link = actions.find((entry) => entry.action.link)?.action.link;
  if (actions.length === 0) return null;
  return (
    <span className="rule-action">
      {link && (
        <a className="link-btn rule-action__link" href={link.href}>
          {link.label}
        </a>
      )}
      <InfoTip label={`O que fazer: ${ruleSentence(result)}`} align="end">
        {actions.map(({ key, title, action }) => (
          <span key={key} className="rule-action__text">
            {title && <strong>{title}. </strong>}
            {action.text}
          </span>
        ))}
      </InfoTip>
    </span>
  );
}

function useColumns(withAction: boolean): DataColumn<RuleResult>[] {
  const money = useMoney();
  const columns: DataColumn<RuleResult>[] = [
    {
      key: "rule",
      label: "Regra",
      rowHeader: true,
      cell: (result) => ruleSentence(result),
    },
    {
      key: "status",
      label: "Estado",
      width: "128px",
      cell: (result) => <StatusPill status={PILL[result.status]}>{STATUS_TEXT[result.status]}</StatusPill>,
    },
    {
      key: "diagnosis",
      label: "Diagnóstico",
      cell: (result) => <Diagnosis result={result} money={money} />,
    },
    {
      key: "now",
      label: "Agora",
      align: "end",
      width: "112px",
      cell: (result) => {
        const now = nowValue(result);
        if (now === null) return null;
        return (
          <span className="rule-now">
            <span className="rule-now__value">{now}</span>
            {showsBar(result) && (
              <LimitBar
                current={result.current}
                limit={result.limit}
                label={`${ruleName(result.rule)}: ${formatRuleValue("percent", result.current)}, limite ${formatRuleValue("percent", result.limit)}`}
              />
            )}
          </span>
        );
      },
    },
    {
      key: "limit",
      label: "Limite",
      align: "end",
      width: "104px",
      cell: (result) => {
        const limit = limitCell(result);
        return limit === null ? null : <span className="rule-limit">{limit}</span>;
      },
    },
  ];
  if (withAction) {
    columns.push({
      key: "action",
      label: "O que fazer",
      align: "end",
      cell: (result) => <Action result={result} money={money} />,
    });
  }
  return columns;
}

type RuleTableProps = {
  caption: string;
  rules: readonly RuleResult[];
  /** A coluna "O que fazer" (só nos pontos a ver). */
  withAction?: boolean;
};

/**
 * As regras de uma dimensão numa tabela compacta: a frase com o seu limite, o
 * estado em texto (a cor só reforça), o diagnóstico em uma linha, o número de
 * agora com a barra do limite e, num ponto a ver, o caminho do que fazer. Sem
 * dado é dito como sem dado, nunca como "tudo certo" nem como falha.
 */
export function RuleTable({ caption, rules, withAction = false }: Readonly<RuleTableProps>) {
  const columns = useColumns(withAction);
  return (
    <DataTable
      caption={caption}
      className="rule-table"
      columns={columns}
      rows={rules}
      rowKey={(result) => result.rule}
      rowClassName={(result) => `analysis-rule analysis-rule--${result.status}`}
      cozy={withAction}
    />
  );
}
