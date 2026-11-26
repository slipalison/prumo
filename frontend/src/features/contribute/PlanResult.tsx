import { motion, useReducedMotion } from "motion/react";
import { useEffect, useState, type ReactNode } from "react";
import type { Asset, AssetClass, Plan, PlanNotice } from "../../api/types";
import { AssetIcon } from "../../components/AssetIcon";
import { ClassBar } from "../../components/ClassBar";
import { CountUp } from "../../components/CountUp";
import { Donut } from "../../components/Donut";
import { IconAlert, IconArrowRight, IconCheck, IconClose, IconInfo, IconPlus } from "../../components/icons";
import { Grid, Panel } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Toggletip } from "../../components/Toggletip";
import { CLASS_META, classColor } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { formatPercent, plural } from "../../lib/format";
import { NO_CEILING_TIP, noticeText, referenceChecks, unitLine } from "../../lib/planText";
import { useMoney } from "../../lib/privacy";
import { assetName, findAssetByLabel, isAboveCeiling } from "../../lib/rating";
import { ItemIncome, PlanIncomeLine } from "../income/PlanIncomeLine";
import { DURATION, EASE_OUT, SPRING, STAGGER } from "../../lib/motion";

type AssetActions = {
  /** Abre a folha do ativo no checklist, para dar nota. */
  onRateAsset: (asset: Asset) => void;
  /** Abre a folha do ativo como ela é (quantidade, preço, B3). */
  onEditAsset: (asset: Asset) => void;
  /** Abre o cadastro de ativo já na classe. */
  onAddAsset: (assetClass: AssetClass) => void;
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
  show: { transition: { staggerChildren: STAGGER, delayChildren: DURATION.instant } },
};

const card = {
  hidden: { opacity: 0, y: 12, scale: 0.97, filter: "blur(4px)" },
  show: { opacity: 1, y: 0, scale: 1, filter: "blur(0px)", transition: SPRING.enter },
};

type PlanResultProps = AssetActions & {
  plan: Plan;
  /** Os ativos da carteira, para cada aviso achar quem abrir. */
  assets: Asset[];
  onConfirm: () => void;
  /** A terceira coluna do que fica abaixo (os objetivos); some se não vier nada. */
  children?: ReactNode;
};

/**
 * O resultado do aporte, em uma região só: o que comprar (a manchete, a renda
 * que soma e um cartão por ativo) ao lado do campo do valor e, abaixo, os avisos,
 * a carteira antes e depois e os objetivos. A região não tem caixa própria
 * (`display: contents`): os filhos entram direto na grade da tela. "Já comprei"
 * fica no cabeçalho do painel; no celular ele flutua no rodapé (contribute.css).
 */
export function PlanResult({ plan, assets, onConfirm, children, ...actions }: PlanResultProps) {
  const money = useMoney();
  const count = plan.items.length;
  const noTargets = plan.notices.some((n) => n.code === "no_targets");
  const missingAssets = plan.notices.some((n) => n.code === "class_blocked");

  const title =
    count > 0 ? (
      <span aria-live="polite">
        Compre <span className="gold">{count}</span> {plural(count, ["ativo", "ativos"])}
      </span>
    ) : (
      <span aria-live="polite">
        {noTargets ? "Defina suas metas primeiro" : missingAssets ? "Faltam ativos onde o aporte precisa ir" : "Nada a comprar agora"}
      </span>
    );

  return (
    <section className="plan-region" aria-label="Resultado do aporte">
      <motion.div
        className="plan-top"
        initial={{ opacity: 0, y: 12 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: -6 }}
        transition={{ duration: DURATION.base, ease: EASE_OUT }}
      >
        <Panel
          className="plan"
          title={title}
        >
          {count > 0 ? (
            <>
              <div className="plan__headline">
                <p className="plan__subtitle">
                  com <CountUp value={plan.invested} className="gold strong" />
                  {plan.change >= 0.01 && <span className="muted"> · troco de {money.brl(plan.change)}</span>}
                </p>
                {plan.income && <PlanIncomeLine income={plan.income} />}
              </div>
              <motion.div variants={list} initial="hidden" animate="show">
                <Grid as="ol" cols="auto" minWidth="180px" className="plan__items">
                  {plan.items.map((item) => (
                    <motion.li
                      key={item.asset_id}
                      variants={card}
                      className="plan-card"
                      style={{ ["--class-color" as string]: classColor(item.asset_class) }}
                    >
                      <span className="plan-card__stripe" aria-hidden="true" />
                      <span className="plan-card__class">{CLASS_META[item.asset_class].shortLabel}</span>
                      <div className="plan-card__title">
                        <AssetIcon symbol={item.symbol} name={item.display_name} assetClass={item.asset_class} treasury={item.treasury} />
                        <strong className={cx("plan-card__symbol", (!item.symbol || item.treasury) && "plan-card__symbol--name")}>
                          {assetName(item)}
                        </strong>
                      </div>
                      {item.symbol && !item.treasury && <span className="plan-card__name">{item.display_name}</span>}
                      <div className="plan-card__foot">
                        <CountUp value={item.value} className="plan-card__value" />
                        <span className="plan-card__units">{unitLine(item, money)}</span>
                        <ItemIncome monthly={item.estimated_monthly_income} />
                      </div>
                      {item.price_references && (
                        <div className="price-refs" role="note" aria-label={`Referências de preço de ${assetName(item)}`}>
                          <span className="price-refs__label">
                            Sem teto
                            <Toggletip label="O que são estas referências" content={NO_CEILING_TIP} className="price-refs__info" align="stretch">
                              <IconInfo width={16} height={16} />
                            </Toggletip>
                          </span>
                          <ul className="price-refs__list">
                            {referenceChecks(item, item.price_references, money).map((check) => (
                              <li key={check.name}>
                                <Toggletip
                                  label={check.label}
                                  content={check.tip}
                                  align="stretch"
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
                    </motion.li>
                  ))}
                </Grid>
              </motion.div>
            </>
          ) : (
            <p className="plan__subtitle muted">
              {noTargets
                ? "As metas dizem quanto você quer em cada classe."
                : missingAssets
                  ? "As classes abaixo da meta não têm ativo pronto para receber. Cadastre o que você quer ter — mesmo sem ter comprado — e eu divido pelas notas."
                  : "Nenhum ativo com nota está abaixo da meta — ou o valor não compra uma unidade."}
            </p>
          )}
          {plan.unallocated >= 0.01 && (
            <p className="plan__unallocated">
              <IconAlert /> {money.brl(plan.unallocated)} ficaram sem destino.
            </p>
          )}
        </Panel>
        {count > 0 && (
          // Fora do painel de vidro: o `backdrop-filter` dele prenderia a doca
          // flutuante do celular dentro do painel (contribute.css).
          <div className="plan__actions">
            <MagneticButton onClick={onConfirm} className="wide">
              <IconCheck /> Já comprei
            </MagneticButton>
          </div>
        )}
      </motion.div>

      <PlanBelow plan={plan} assets={assets} {...actions}>
        {children}
      </PlanBelow>
    </section>
  );
}

type PlanBelowProps = AssetActions & {
  plan: Plan;
  /** Os ativos da carteira, para cada aviso achar quem abrir. */
  assets: Asset[];
  /** A terceira coluna (os objetivos); some se não vier nada. */
  children?: ReactNode;
};

/**
 * O que fica abaixo do resultado, lado a lado: os avisos (uma linha cada, com a
 * ação junto), a carteira antes e depois e, vindos de fora, os objetivos.
 */
export function PlanBelow({ plan, assets, children, ...actions }: PlanBelowProps) {
  const reduce = useReducedMotion();
  const money = useMoney();
  const [view, setView] = useState<"before" | "after">(reduce ? "after" : "before");

  // A rosca nasce no "antes" e escorrega para o "depois" sozinha.
  useEffect(() => {
    if (reduce) return undefined;
    const timer = window.setTimeout(() => setView("after"), 400);
    return () => window.clearTimeout(timer);
  }, [reduce, plan]);

  const shownClasses = plan.classes.filter((c) => c.target_percent > 0 || c.percent_before > 0);
  const now = view === "before" ? "hoje" : "depois do aporte";

  return (
    <motion.div
      className="plan-below-wrap"
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0 }}
      transition={{ duration: DURATION.base, ease: EASE_OUT, delay: DURATION.instant }}
    >
      <Grid cols="auto" minWidth="340px" alignStart className="plan-below">
      {plan.notices.length > 0 && (
        <Panel title={`Avisos (${plan.notices.length})`} className="plan-notices">
          <ul className="notices">
            {plan.notices.map((notice, index) => (
              <motion.li
                key={`${notice.code}-${index}`}
                className={cx("notice", `notice--${notice.code}`)}
                initial={{ opacity: 0, x: -10 }}
                animate={{ opacity: 1, x: 0 }}
                transition={{ duration: DURATION.base, delay: DURATION.instant + index * STAGGER }}
              >
                <IconAlert className="notice__icon" />
                <span className="notice__text">{noticeText(notice, assets, money)}</span>
                <span className="notice__action">
                  <NoticeAction notice={notice} assets={assets} {...actions} />
                </span>
              </motion.li>
            ))}
          </ul>
        </Panel>
      )}

      {shownClasses.length > 0 && (
        <Panel
          className="plan-balance"
          title={`A carteira ${now}`}
          infoLabel="Como ler a carteira antes e depois"
          info="Cada barra é uma classe. O traço com o peso marca a meta; o trecho dourado é o que este aporte soma."
          actions={
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
          }
        >
          <div className="plan-balance__body">
            <Donut
              size={132}
              thickness={12}
              label={`Carteira ${now}`}
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
        </Panel>
      )}

      {children}
      </Grid>
    </motion.div>
  );
}
