import { motion, useReducedMotion } from "motion/react";
import { useEffect, useState } from "react";
import type { Asset, AssetClass, Plan, PlanNotice } from "../../api/types";
import { AssetIcon } from "../../components/AssetIcon";
import { ClassBar } from "../../components/ClassBar";
import { CountUp } from "../../components/CountUp";
import { Donut } from "../../components/Donut";
import { GlassCard } from "../../components/GlassCard";
import { IconAlert, IconArrowRight, IconCheck, IconClose, IconInfo, IconPlus } from "../../components/icons";
import { MagneticButton } from "../../components/MagneticButton";
import { Toggletip } from "../../components/Toggletip";
import { CLASS_META, classColor } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { formatPercent, plural } from "../../lib/format";
import { NO_CEILING_TIP, noticeText, referenceChecks, unitLine } from "../../lib/planText";
import { useMoney } from "../../lib/privacy";
import { assetName, findAssetByLabel, isAboveCeiling } from "../../lib/rating";
import { ItemIncome, PlanIncomeLine } from "../income/PlanIncomeLine";

type AssetActions = {
  /** Abre a folha do ativo no checklist, para dar nota. */
  onRateAsset: (asset: Asset) => void;
  /** Abre a folha do ativo como ela é (quantidade, preço, B3). */
  onEditAsset: (asset: Asset) => void;
  /** Abre o cadastro de ativo já na classe. */
  onAddAsset: (assetClass: AssetClass) => void;
};

type PlanResultProps = AssetActions & {
  plan: Plan;
  /** Os ativos da carteira, para cada aviso achar quem abrir. */
  assets: Asset[];
  onConfirm: () => void;
};

/** Onde fica o limite por ativo: no fim de Metas por classe, em Ajustes. */
const LIMIT_HREF = "#/ajustes/metas";

function single(list: Array<Asset | undefined>): Asset | undefined {
  const found = list.filter((a): a is Asset => a !== undefined);
  return found.length === 1 ? found[0] : undefined;
}

/** A ação de cada aviso: abre o que resolve, sem a pessoa procurar. */
function NoticeAction({ notice, assets, onRateAsset, onEditAsset, onAddAsset }: AssetActions & { notice: PlanNotice; assets: Asset[] }) {
  const arrow = <IconArrowRight width={16} height={16} />;
  const rate = (asset: Asset | undefined) =>
    asset && (
      <button type="button" className="link-btn" onClick={() => onRateAsset(asset)}>
        Dar nota ao {assetName(asset)} {arrow}
      </button>
    );
  const open = (asset: Asset | undefined) =>
    asset && (
      <button type="button" className="link-btn" onClick={() => onEditAsset(asset)}>
        Abrir {assetName(asset)} {arrow}
      </button>
    );
  const bySymbol = (symbols: string[]) => single(symbols.map((s) => findAssetByLabel(assets, s)));

  const adjustLimit = (
    <a className="link-btn" href={LIMIT_HREF}>
      Ajustar o limite {arrow}
    </a>
  );

  if (notice.code === "no_targets") {
    return (
      <a className="link-btn" href="#/ajustes/metas">
        Ir para Metas {arrow}
      </a>
    );
  }
  if (notice.code === "concentration_limit") return adjustLimit;
  if (notice.code === "asset_above_ceiling") {
    const action = open(assets.find((a) => a.id === notice.asset_id) ?? findAssetByLabel(assets, notice.symbol));
    if (action) return action;
  }
  if (notice.code === "class_blocked") {
    if (notice.reason === "no_assets") {
      return (
        <button type="button" className="link-btn" onClick={() => onAddAsset(notice.asset_class)}>
          <IconPlus width={16} height={16} /> Adicionar em {CLASS_META[notice.asset_class].shortLabel}
        </button>
      );
    }
    if (notice.reason === "concentration_limit") return adjustLimit;
    const members = assets.filter((a) => a.asset_class === notice.asset_class);
    const action =
      notice.reason === "unrated"
        ? rate(single(members.filter((a) => a.score <= 0)))
        : notice.reason === "above_ceiling"
          ? open(single(members.filter((a) => a.score > 0 && isAboveCeiling(a))))
          : open(single(members.filter((a) => a.score > 0 && a.price_source === "missing")));
    if (action) return action;
  }
  if (notice.code === "assets_without_score") {
    const action = rate(bySymbol(notice.symbols));
    if (action) return action;
  }
  if (notice.code === "assets_without_price") {
    const action = open(bySymbol(notice.symbols));
    if (action) return action;
  }
  return (
    <a className="link-btn" href="#/carteira">
      Abrir Carteira <IconArrowRight width={16} height={16} />
    </a>
  );
}

const list = {
  hidden: {},
  show: { transition: { staggerChildren: 0.06, delayChildren: 0.08 } },
};

const card = {
  hidden: { opacity: 0, y: 18, scale: 0.97, filter: "blur(6px)" },
  show: { opacity: 1, y: 0, scale: 1, filter: "blur(0px)", transition: { type: "spring" as const, stiffness: 260, damping: 24 } },
};

export function PlanResult({ plan, assets, onConfirm, ...actions }: PlanResultProps) {
  const reduce = useReducedMotion();
  const money = useMoney();
  const [view, setView] = useState<"before" | "after">(reduce ? "after" : "before");

  // A rosca nasce no "antes" e escorrega para o "depois" sozinha.
  useEffect(() => {
    if (reduce) return undefined;
    const timer = window.setTimeout(() => setView("after"), 650);
    return () => window.clearTimeout(timer);
  }, [reduce, plan]);

  const count = plan.items.length;
  const shownClasses = plan.classes.filter((c) => c.target_percent > 0 || c.percent_before > 0);
  const noTargets = plan.notices.some((n) => n.code === "no_targets");
  const missingAssets = plan.notices.some((n) => n.code === "class_blocked");

  return (
    <section className="plan" aria-label="Resultado do aporte">
      <div className="plan__headline" aria-live="polite">
        {count > 0 ? (
          <>
            <h2 className="plan__title">
              Compre <span className="gold">{count}</span> {plural(count, ["ativo", "ativos"])}
            </h2>
            <p className="plan__subtitle">
              com <CountUp value={plan.invested} className="gold strong" />
              {plan.change >= 0.01 && <span className="muted"> · troco de {money.brl(plan.change)}</span>}
            </p>
            {plan.income && <PlanIncomeLine income={plan.income} />}
          </>
        ) : (
          <>
            <h2 className="plan__title">
              {noTargets
                ? "Defina suas metas primeiro"
                : missingAssets
                  ? "Faltam ativos onde o aporte precisa ir"
                  : "Nada a comprar agora"}
            </h2>
            <p className="plan__subtitle muted">
              {noTargets
                ? "As metas dizem quanto você quer em cada classe."
                : missingAssets
                  ? "As classes abaixo da meta não têm ativo pronto para receber. Cadastre o que você quer ter — mesmo sem ter comprado — e eu divido pelas notas."
                  : "Nenhum ativo com nota está abaixo da meta — ou o valor não compra uma unidade."}
            </p>
          </>
        )}
        {plan.unallocated >= 0.01 && (
          <p className="plan__unallocated">
            <IconAlert /> {money.brl(plan.unallocated)} ficaram sem destino.
          </p>
        )}
      </div>

      {count > 0 && (
        <motion.ol className="plan__items" variants={list} initial="hidden" animate="show">
          {plan.items.map((item) => (
            <motion.li key={item.asset_id} variants={card}>
              <GlassCard
                className="plan-card"
                style={{ ["--class-color" as string]: classColor(item.asset_class) }}
              >
                <span className="plan-card__stripe" aria-hidden="true" />
                <div className="plan-card__head">
                  <span className="plan-card__class">{CLASS_META[item.asset_class].shortLabel}</span>
                  <div className="plan-card__title">
                    <AssetIcon symbol={item.symbol} name={item.display_name} assetClass={item.asset_class} treasury={item.treasury} />
                    <strong className={cx("plan-card__symbol", (!item.symbol || item.treasury) && "plan-card__symbol--name")}>
                      {assetName(item)}
                    </strong>
                  </div>
                  {item.symbol && !item.treasury && <span className="plan-card__name">{item.display_name}</span>}
                </div>
                <div className="plan-card__foot">
                  <CountUp value={item.value} className="plan-card__value" />
                  <span className="plan-card__units">{unitLine(item, money)}</span>
                </div>
                <ItemIncome monthly={item.estimated_monthly_income} />
                {item.price_references && (
                  <div className="price-refs" role="note" aria-label={`Referências de preço de ${assetName(item)}`}>
                    <span className="price-refs__label">
                      Sem teto
                      <Toggletip label="O que são estas referências" content={NO_CEILING_TIP} className="price-refs__info">
                        <IconInfo width={16} height={16} />
                      </Toggletip>
                    </span>
                    <ul className="price-refs__list">
                      {referenceChecks(item, item.price_references, money).map((check) => (
                        <li key={check.name}>
                          <Toggletip
                            label={check.label}
                            content={check.tip}
                            className={cx("price-ref", check.approves ? "price-ref--approves" : "price-ref--rejects")}
                          >
                            {check.approves ? <IconCheck width={15} height={15} /> : <IconClose width={15} height={15} />}
                            <span>{check.name}</span>
                            <span className="price-ref__value">{money.brl(check.value)}</span>
                          </Toggletip>
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
              </GlassCard>
            </motion.li>
          ))}
        </motion.ol>
      )}

      {plan.notices.length > 0 && (
        <ul className="notices">
          {plan.notices.map((notice, index) => (
            <motion.li
              key={`${notice.code}-${index}`}
              className={cx("notice glass", `notice--${notice.code}`)}
              initial={{ opacity: 0, x: -10 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ delay: 0.2 + index * 0.05 }}
            >
              <IconAlert className="notice__icon" />
              <span>{noticeText(notice, assets, money)}</span>
              <NoticeAction notice={notice} assets={assets} {...actions} />
            </motion.li>
          ))}
        </ul>
      )}

      {shownClasses.length > 0 && (
        <GlassCard as="section" className="plan__balance" aria-label="Carteira antes e depois">
          <div className="plan__balance-head">
            <h3 className="section-title">A carteira {view === "before" ? "hoje" : "depois do aporte"}</h3>
            <div className="segmented" role="group" aria-label="Comparar">
              {(["before", "after"] as const).map((option) => (
                <button
                  key={option}
                  type="button"
                  className={cx("segmented__option", view === option && "is-active")}
                  aria-pressed={view === option}
                  onClick={() => setView(option)}
                >
                  {view === option && <motion.span layoutId="plan-view" className="segmented__knob" />}
                  <span className="segmented__label">{option === "before" ? "Antes" : "Depois"}</span>
                </button>
              ))}
            </div>
          </div>
          <div className="plan__balance-body">
            <Donut
              size={180}
              thickness={16}
              label={`Carteira ${view === "before" ? "hoje" : "depois do aporte"}`}
              segments={plan.classes.map((c) => ({
                key: c.asset_class,
                value: view === "before" ? c.percent_before : c.percent_after,
                color: classColor(c.asset_class),
                label: CLASS_META[c.asset_class].label,
              }))}
            >
              <span className="donut__caption">{view === "before" ? "hoje" : "depois"}</span>
              <strong className="donut__number donut__number--sm">
                {money.compact(view === "before" ? plan.total_before : plan.total_after)}
              </strong>
            </Donut>
            <ul className="plan__bars">
              {shownClasses.map((c) => (
                <li key={c.asset_class} className="plan__bar">
                  <div className="plan__bar-head">
                    <span className="plan__bar-label">
                      <span className="dot" style={{ background: classColor(c.asset_class) }} aria-hidden="true" />
                      {CLASS_META[c.asset_class].label}
                    </span>
                    <span className="plan__bar-numbers">
                      {formatPercent(c.percent_before)}
                      {c.allocated > 0 && (
                        <>
                          {" → "}
                          <span className="gold">{formatPercent(c.percent_after)}</span>
                        </>
                      )}
                      <span className="muted"> / {formatPercent(c.target_percent)}</span>
                    </span>
                  </div>
                  <ClassBar
                    assetClass={c.asset_class}
                    current={c.percent_before}
                    target={c.target_percent}
                    after={view === "after" ? c.percent_after : c.percent_before}
                  />
                </li>
              ))}
            </ul>
          </div>
        </GlassCard>
      )}

      {count > 0 && (
        <div className="plan__actions">
          <MagneticButton onClick={onConfirm} className="wide">
            <IconCheck /> Já comprei
          </MagneticButton>
        </div>
      )}
    </section>
  );
}
