import type { HealthWarning } from "../../api/healthTypes";
import { AssetIcon } from "../../components/AssetIcon";
import { LimitBar } from "../../components/LimitBar";
import { iconSubjectOfLabel } from "../../lib/assetIcon";
import {
  formatRuleValue,
  limitText,
  warningAction,
  warningAssets,
  warningMembers,
  warningTitle,
} from "../../lib/healthText";
import { useMoney } from "../../lib/privacy";

/**
 * Um achado de uma das sete regras originais da Saúde: quem passou do limite,
 * o número de agora e o que fazer (aportar fora do que passou; nunca vender).
 */
export function Finding({ warning }: { warning: HealthWarning }) {
  const money = useMoney();
  const action = warningAction(warning, money);
  const members = warningMembers(warning);
  const assets = warningAssets(warning);
  const title = warningTitle(warning);
  const minimum =
    warning.rule === "min_assets" || warning.rule === "empty_class" || warning.rule === "income_every_month";

  return (
    <article className="finding" aria-label={title}>
      <h4 className="finding__title">{title}</h4>
      {assets.length > 0 && (
        <ul className="warning-card__assets" aria-hidden="true">
          {assets.map((asset) => (
            <li key={asset.label}>
              <AssetIcon size="sm" assetClass={asset.assetClass} {...iconSubjectOfLabel(asset.label)} />
            </li>
          ))}
        </ul>
      )}
      {members && <p className="field__hint">{members}</p>}
      <dl className="warning-card__numbers finding__numbers">
        <div>
          <dt>Agora</dt>
          <dd>{formatRuleValue(warning.unit, warning.current)}</dd>
        </div>
        <div>
          <dt>{minimum ? "Mínimo" : "Limite"}</dt>
          <dd>{formatRuleValue(warning.unit, warning.limit)}</dd>
        </div>
      </dl>
      {warning.unit === "percent" && (
        <LimitBar current={warning.current} limit={warning.limit} label={`${title}; ${limitText(warning).toLowerCase()}`} />
      )}
      <p className="warning-card__action">
        <span className="warning-card__what">O que fazer.</span> {action.text}
      </p>
      {action.link && (
        <a className="link-btn" href={action.link.href}>
          {action.link.label}
        </a>
      )}
    </article>
  );
}
