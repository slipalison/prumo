import { motion } from "motion/react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { Asset, AssetClass, Plan, PlanItem, PlanNotice } from "../../api/types";
import { AssetIcon } from "../../components/AssetIcon";
import { CountUp } from "../../components/CountUp";
import { IconAlert, IconArrowRight, IconCheck, IconClose, IconInfo, IconPlus } from "../../components/icons";
import { Grid, Panel } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Toggletip } from "../../components/Toggletip";
import { CLASS_META, classColor } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { plural } from "../../lib/format";
import { NO_CEILING_TIP, noticeText, purchaseLine, referenceChecks, unitLine } from "../../lib/planText";
import { useMoney } from "../../lib/privacy";
import { assetName, findAssetByLabel, isAboveCeiling, isCeilingUnavailable } from "../../lib/rating";
import { ItemIncome, PlanIncomeLine } from "../income/PlanIncomeLine";
import { DURATION, EASE_OUT, SPRING, STAGGER } from "../../lib/motion";
import { BalancePanel } from "./BalancePanel";
import { BoughtSheet } from "./BoughtSheet";
import { differsFromPlan, markValue, type Mark, type Marks } from "./marks";

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

/** O que resolve cada aviso: uma tela, o cadastro na classe ou a folha de um ativo. */
type NoticeTarget =
  | { kind: "goals" }
  | { kind: "limit" }
  | { kind: "add"; assetClass: AssetClass }
  | { kind: "rate" | "open"; asset: Asset }
  | { kind: "portfolio" };

const PORTFOLIO_TARGET: NoticeTarget = { kind: "portfolio" };

/** Dar nota ou abrir o ativo, quando há um só a abrir; sem ele, a Carteira. */
function assetTarget(kind: "rate" | "open", asset: Asset | undefined): NoticeTarget {
  return asset ? { kind, asset } : PORTFOLIO_TARGET;
}

/** A classe abaixo da meta que não recebe: o que a destrava, pelo motivo. */
function blockedClassTarget(notice: Extract<PlanNotice, { code: "class_blocked" }>, assets: Asset[]): NoticeTarget {
  if (notice.reason === "no_assets") return { kind: "add", assetClass: notice.asset_class };
  if (notice.reason === "concentration_limit") return { kind: "limit" };
  const members = assets.filter((a) => a.asset_class === notice.asset_class);
  if (notice.reason === "unrated") return assetTarget("rate", single(members.filter((a) => a.score <= 0)));
  const rated = members.filter((a) => a.score > 0);
  if (notice.reason === "above_ceiling") return assetTarget("open", single(rated.filter((a) => isAboveCeiling(a))));
  if (notice.reason === "ceiling_unavailable") return assetTarget("open", single(rated.filter((a) => isCeilingUnavailable(a))));
  return assetTarget("open", single(rated.filter((a) => a.price_source === "missing")));
}

function noticeTarget(notice: PlanNotice, assets: Asset[]): NoticeTarget {
  const bySymbol = (symbols: string[]) => single(symbols.map((s) => findAssetByLabel(assets, s)));
  switch (notice.code) {
    case "no_targets":
      return { kind: "goals" };
    case "concentration_limit":
      return { kind: "limit" };
    case "asset_above_ceiling":
    case "asset_ceiling_unavailable":
      return assetTarget("open", assets.find((a) => a.id === notice.asset_id) ?? findAssetByLabel(assets, notice.symbol));
    case "class_blocked":
      return blockedClassTarget(notice, assets);
    case "assets_without_score":
      return assetTarget("rate", bySymbol(notice.symbols));
    case "assets_without_price":
      return assetTarget("open", bySymbol(notice.symbols));
    default:
      return PORTFOLIO_TARGET;
  }
}

/** A ação de cada aviso: abre o que resolve, sem a pessoa procurar. */
function NoticeAction({
  notice,
  assets,
  onRateAsset,
  onEditAsset,
  onAddAsset,
}: Readonly<AssetActions & { notice: PlanNotice; assets: Asset[] }>) {
  const arrow = <IconArrowRight width={16} height={16} />;
  const target = noticeTarget(notice, assets);

  switch (target.kind) {
    case "goals":
      return (
        <a className="link-btn" href="#/ajustes/metas">
          Ir para Metas {arrow}
        </a>
      );
    case "limit":
      return (
        <a className="link-btn" href={LIMIT_HREF}>
          Ajustar o limite {arrow}
        </a>
      );
    case "add":
      return (
        <button type="button" className="link-btn" onClick={() => onAddAsset(target.assetClass)}>
          <IconPlus width={16} height={16} /> Adicionar em {CLASS_META[target.assetClass].shortLabel}
        </button>
      );
    case "rate":
      return (
        <button type="button" className="link-btn" onClick={() => onRateAsset(target.asset)}>
          Dar nota ao {assetName(target.asset)} {arrow}
        </button>
      );
    case "open":
      return (
        <button type="button" className="link-btn" onClick={() => onEditAsset(target.asset)}>
          Abrir {assetName(target.asset)} {arrow}
        </button>
      );
    default:
      return (
        <a className="link-btn" href="#/carteira">
          Abrir Carteira {arrow}
        </a>
      );
  }
}

/** Plano sem compra: a manchete e o porquê, do motivo mais forte ao mais comum. */
function emptyPlanText(noTargets: boolean, missingAssets: boolean): { title: string; detail: string } {
  if (noTargets) {
    return { title: "Defina suas metas primeiro", detail: "As metas dizem quanto você quer em cada classe." };
  }
  if (missingAssets) {
    return {
      title: "Faltam ativos onde o aporte precisa ir",
      detail:
        "As classes abaixo da meta não têm ativo pronto para receber. Cadastre o que você quer ter — mesmo sem ter comprado — e eu divido pelas notas.",
    };
  }
  return {
    title: "Nada a comprar agora",
    detail: "Nenhum ativo com nota está abaixo da meta — ou o valor não compra uma unidade.",
  };
}

/** Chave de um aviso pelo conteúdo: o plano não repete aviso (um por classe, por ativo ou por tipo). */
const noticeKey = (notice: PlanNotice) => JSON.stringify(notice);

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
  /** O que a pessoa marcou como comprado, por ativo do plano. */
  marks: Marks;
  /** Marca (ou corrige a marca de) um ativo como comprado, com a quantidade e o preço pagos. */
  onMark: (item: PlanItem, mark: Mark) => void;
  /** Tira a marca de um ativo. */
  onUnmark: (item: PlanItem) => void;
  /** Marca como comprado, como o plano sugere, todo ativo que ainda não está marcado. */
  onMarkAll: () => void;
  /** Abre a conferência para registrar os marcados. */
  onConfirm: () => void;
  /** A terceira coluna do que fica abaixo (os objetivos); some se não vier nada. */
  children?: ReactNode;
};

/**
 * O resultado do aporte, em uma região só, sem caixa própria (`display:
 * contents`): os filhos entram direto na grade da tela. Na linha de cima, a
 * carteira antes e depois ao lado do campo do valor; embaixo, na largura
 * inteira, o que comprar (um cartão por ativo, cada um com o seu "Comprei") e,
 * depois, os avisos e os objetivos. "Registrar N comprados" fica no cabeçalho
 * do painel; no celular ele flutua no rodapé (contribute.css).
 */
export function PlanResult({ plan, assets, marks, onMark, onUnmark, onMarkAll, onConfirm, children, ...actions }: PlanResultProps) {
  const money = useMoney();
  const count = plan.items.length;
  const noTargets = plan.notices.some((n) => n.code === "no_targets");
  const missingAssets = plan.notices.some((n) => n.code === "class_blocked");
  // A folha "Comprei": de qual ativo e se está aberta (fechada, o ativo fica para a saída animada).
  const [asking, setAsking] = useState<{ item: PlanItem; open: boolean } | null>(null);
  // Fechada a folha, o foco vai para o botão do cartão que a pessoa acabou de
  // mexer: o "Comprei" some ao marcar e o "Editar" nasce no lugar (e vice-versa).
  const refocus = useRef<string | null>(null);

  useEffect(() => {
    const id = refocus.current;
    if (!id) return;
    refocus.current = null;
    document.querySelector<HTMLElement>(`[data-mark-action="${id}"]`)?.focus({ preventScroll: true });
  }, [marks]);

  const marked = plan.items.filter((item) => marks[item.asset_id] !== undefined).length;
  const left = count - marked;
  const empty = emptyPlanText(noTargets, missingAssets);

  const title =
    count > 0 ? (
      <span aria-live="polite">
        Compre <span className="gold">{count}</span> {plural(count, ["ativo", "ativos"])}
      </span>
    ) : (
      <span aria-live="polite">{empty.title}</span>
    );

  return (
    <section className="plan-region" aria-label="Resultado do aporte">
      <BalancePanel classes={plan.classes} totalBefore={plan.total_before} totalAfter={plan.total_after} mode="plan" planKey={plan} />

      <motion.div
        className="plan-top"
        initial={{ opacity: 0, y: 12 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: -6 }}
        transition={{ duration: DURATION.base, ease: EASE_OUT }}
      >
        <Panel className="plan" title={title}>
          {count > 0 ? (
            <>
              <div className="plan__headline">
                <div className="plan__headline-row">
                  <p className="plan__subtitle">
                    com <CountUp value={plan.invested} className="gold strong" />
                    {plan.change >= 0.01 && <span className="muted"> · troco de {money.brl(plan.change)}</span>}
                  </p>
                  <div className="plan__marks">
                    {marked > 0 && (
                      <p className="plan__marks-status" aria-live="polite">
                        {marked} de {count} marcados
                        {left > 0 && ` · ${left} ${plural(left, ["fica", "ficam"])} de fora deste registro`}
                      </p>
                    )}
                    {left > 0 && (
                      <button type="button" className="link-btn" onClick={onMarkAll}>
                        <IconCheck width={16} height={16} /> {marked === 0 ? "Comprei todos como sugerido" : "Marcar os demais como sugerido"}
                      </button>
                    )}
                  </div>
                </div>
                {plan.income && <PlanIncomeLine income={plan.income} />}
              </div>
              <motion.div variants={list} initial="hidden" animate="show">
                <Grid as="ol" cols="auto" minWidth="200px" className="plan__items">
                  {plan.items.map((item) => {
                    const mark = marks[item.asset_id];
                    const different = mark !== undefined && differsFromPlan(item, mark);
                    const name = assetName(item);
                    return (
                      <motion.li
                        key={item.asset_id}
                        variants={card}
                        className={cx("plan-card", mark && "is-marked", different && "is-different")}
                        style={{ ["--class-color" as string]: classColor(item.asset_class) }}
                      >
                        <span className="plan-card__stripe" aria-hidden="true" />
                        <span className="plan-card__class">{CLASS_META[item.asset_class].shortLabel}</span>
                        <div className="plan-card__title">
                          <AssetIcon symbol={item.symbol} name={item.display_name} assetClass={item.asset_class} treasury={item.treasury} />
                          <strong className={cx("plan-card__symbol", (!item.symbol || item.treasury) && "plan-card__symbol--name")}>{name}</strong>
                        </div>
                        {item.symbol && !item.treasury && <span className="plan-card__name">{item.display_name}</span>}
                        {mark ? (
                          <div className="plan-card__foot">
                            <span className="plan-card__value">{money.brl(markValue(mark))}</span>
                            <span className="plan-card__done">
                              <IconCheck width={15} height={15} /> Comprado: {purchaseLine(item, mark, money)}
                            </span>
                            {different && <span className="plan-card__differs">Plano: {purchaseLine(item, item, money)}</span>}
                          </div>
                        ) : (
                          <div className="plan-card__foot">
                            <CountUp value={item.value} className="plan-card__value" />
                            <span className="plan-card__units">{unitLine(item, money)}</span>
                            <ItemIncome monthly={item.estimated_monthly_income} />
                          </div>
                        )}
                        {item.price_references && (
                          <div className="price-refs" role="note" aria-label={`Referências de preço de ${name}`}>
                            <span className="price-refs__label">
                              Sem teto
                              <Toggletip label="O que são estas referências" content={NO_CEILING_TIP} className="price-refs__info" align="stretch">
                                <IconInfo width={16} height={16} />
                              </Toggletip>
                            </span>
                            <ul className="price-refs__list">
                              {referenceChecks(item, item.price_references, money).map((check) => (
                                <li key={check.method}>
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
                        <div className="plan-card__actions">
                          {mark ? (
                            <>
                              <button
                                type="button"
                                className="link-btn"
                                data-mark-action={item.asset_id}
                                aria-label={`Editar a compra de ${name}`}
                                onClick={() => setAsking({ item, open: true })}
                              >
                                Editar
                              </button>
                              <button
                                type="button"
                                className="link-btn link-btn--danger"
                                aria-label={`Desfazer a compra de ${name}`}
                                onClick={() => {
                                  refocus.current = item.asset_id;
                                  onUnmark(item);
                                }}
                              >
                                Desfazer
                              </button>
                            </>
                          ) : (
                            <button
                              type="button"
                              className="btn btn--soft plan-card__buy"
                              data-mark-action={item.asset_id}
                              aria-haspopup="dialog"
                              aria-label={`Comprei ${name}`}
                              onClick={() => setAsking({ item, open: true })}
                            >
                              Comprei
                            </button>
                          )}
                        </div>
                      </motion.li>
                    );
                  })}
                </Grid>
              </motion.div>
            </>
          ) : (
            <p className="plan__subtitle muted">{empty.detail}</p>
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
            <MagneticButton onClick={onConfirm} disabled={marked === 0} className="wide">
              <IconCheck /> {marked === 0 ? "Registrar comprados" : `Registrar ${marked} ${plural(marked, ["comprado", "comprados"])}`}
            </MagneticButton>
          </div>
        )}
      </motion.div>

      <PlanBelow plan={plan} assets={assets} {...actions}>
        {children}
      </PlanBelow>

      {/* Sempre montada: quem a abre precisa dela fechada antes, para a folha
          guardar o foco do botão e devolvê-lo ao fechar. */}
      <BoughtSheet
        open={asking?.open ?? false}
        item={asking?.item ?? null}
        mark={asking ? marks[asking.item.asset_id] : undefined}
        onClose={() => setAsking((current) => current && { ...current, open: false })}
        onConfirm={(mark) => {
          if (!asking) return;
          refocus.current = asking.item.asset_id;
          onMark(asking.item, mark);
          setAsking({ ...asking, open: false });
        }}
      />
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
 * ação junto) e, vindos de fora, os objetivos.
 */
export function PlanBelow({ plan, assets, children, ...actions }: PlanBelowProps) {
  const money = useMoney();

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
                  key={noticeKey(notice)}
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

        {children}
      </Grid>
    </motion.div>
  );
}
