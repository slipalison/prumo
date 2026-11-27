import { motion, useReducedMotion } from "motion/react";
import { useEffect, useState } from "react";
import type { PlanClass } from "../../api/types";
import { ClassBar } from "../../components/ClassBar";
import { Donut } from "../../components/Donut";
import { Panel } from "../../components/Layout";
import { CLASS_META, classColor } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { formatPercent } from "../../lib/format";
import { useMoney } from "../../lib/privacy";

/** O que o painel precisa de cada classe: o plano entrega isso, e a carteira de hoje também. */
export type BalanceClass = Pick<PlanClass, "asset_class" | "target_percent" | "percent_before" | "percent_after" | "allocated">;

type BalancePanelProps = {
  classes: BalanceClass[];
  totalBefore: number;
  totalAfter: number;
  /**
   * `today`: antes de calcular, só como a carteira está (sem "depois", sem
   * alternância). `plan`: com o plano, nasce no "antes" e escorrega para o
   * "depois"; a pessoa alterna.
   */
  mode: "today" | "plan";
  /** O plano na tela: um plano novo escorrega de novo do "antes" para o "depois". */
  planKey?: unknown;
};

/**
 * A carteira por classe, ao lado do campo do valor (docs/layout.md, 7.1): a
 * rosca e uma barra por classe com a meta. Antes do cálculo mostra "hoje"; com
 * o plano, alterna "antes" e "depois do aporte".
 */
export function BalancePanel({ classes, totalBefore, totalAfter, mode, planKey }: Readonly<BalancePanelProps>) {
  const reduce = useReducedMotion();
  const money = useMoney();
  const planned = mode === "plan";
  const [view, setView] = useState<"before" | "after">(planned && reduce ? "after" : "before");

  // A rosca nasce no "antes" e escorrega para o "depois" sozinha.
  useEffect(() => {
    if (!planned || reduce) return undefined;
    const timer = window.setTimeout(() => setView("after"), 400);
    return () => window.clearTimeout(timer);
  }, [planned, reduce, planKey]);

  const shown = classes.filter((c) => c.target_percent > 0 || c.percent_before > 0);
  if (shown.length === 0) return null;

  const now = !planned || view === "before" ? "hoje" : "depois do aporte";

  return (
    <Panel
      className="plan-balance"
      title={`A carteira ${now}`}
      infoLabel="Como ler a carteira"
      info={
        planned
          ? "Cada barra é uma classe. O traço com o peso marca a meta; o trecho dourado é o que este aporte soma."
          : "Cada barra é uma classe. O traço com o peso marca a meta. Calcule um aporte para ver como ela fica depois."
      }
      actions={
        planned ? (
          <fieldset className="segmented" aria-label="Comparar">
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
          </fieldset>
        ) : undefined
      }
    >
      <div className="plan-balance__body">
        <Donut
          size={132}
          thickness={12}
          label={`Carteira ${now}`}
          segments={classes.map((c) => ({
            key: c.asset_class,
            value: view === "before" ? c.percent_before : c.percent_after,
            color: classColor(c.asset_class),
            label: CLASS_META[c.asset_class].label,
          }))}
        >
          <span className="donut__caption">{view === "before" ? "hoje" : "depois"}</span>
          <strong className="donut__number donut__number--sm">{money.compact(view === "before" ? totalBefore : totalAfter)}</strong>
        </Donut>
        <ul className="plan__bars">
          {shown.map((c) => (
            <li key={c.asset_class} className="plan__bar">
              <div className="plan__bar-head">
                <span className="plan__bar-label">
                  <span className="dot" style={{ background: classColor(c.asset_class) }} aria-hidden="true" />
                  {CLASS_META[c.asset_class].label}
                </span>
                <span className="plan__bar-numbers">
                  {formatPercent(c.percent_before)}
                  {planned && c.allocated > 0 && (
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
  );
}
