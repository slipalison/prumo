import { useEffect, useMemo, useState } from "react";
import { errorMessage } from "../../api/client";
import { useSaveGoals } from "../../api/queries";
import type { AssetClass, GoalTargets, Portfolio } from "../../api/types";
import { Donut } from "../../components/Donut";
import { GlassCard } from "../../components/GlassCard";
import { MagneticButton } from "../../components/MagneticButton";
import { useToast } from "../../components/toastContext";
import { ASSET_CLASSES, CLASS_META, classColor } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { formatPercent } from "../../lib/format";
import { balanceTargets, isComplete, PRESETS, sameTargets, sumTargets, targetsFromClasses } from "../../lib/goals";
import { SumRing } from "./SumRing";

type GoalsScreenProps = { portfolio: Portfolio };

export function GoalsScreen({ portfolio }: GoalsScreenProps) {
  const saved = useMemo(() => targetsFromClasses(portfolio.classes), [portfolio.classes]);
  const [draft, setDraft] = useState<GoalTargets>(saved);
  const [touched, setTouched] = useState(false);
  const save = useSaveGoals();
  const toast = useToast();

  // Enquanto a pessoa não mexeu, a tela acompanha o que o servidor diz.
  useEffect(() => {
    if (!touched) setDraft(saved);
  }, [saved, touched]);

  const sum = sumTargets(draft);
  const complete = isComplete(draft);
  const dirty = !sameTargets(draft, saved);
  const current = Object.fromEntries(portfolio.classes.map((c) => [c.asset_class, c.current_percent])) as Record<
    AssetClass,
    number
  >;

  const change = (next: GoalTargets) => {
    setTouched(true);
    setDraft(next);
  };

  const submit = () => {
    save.mutate(draft, {
      onSuccess: () => {
        setTouched(false);
        toast({ tone: "success", message: "Metas salvas. O próximo aporte já segue elas." });
      },
      onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
    });
  };

  return (
    <div className="screen goals">
      <header className="screen__header">
        <p className="eyebrow">Metas</p>
        <h1 className="screen__title">Quanto você quer em cada classe?</h1>
        <p className="screen__lead">Os aportes vão sempre para o que está mais longe da meta.</p>
      </header>

      <div className="preset-grid" role="group" aria-label="Perfis prontos">
        {PRESETS.map((preset) => {
          const active = sameTargets(preset.targets, draft);
          return (
            <button
              key={preset.id}
              type="button"
              aria-pressed={active}
              className={cx("preset-card glass", active && "is-active")}
              onClick={() => change(preset.targets)}
            >
              <strong className="preset-card__title">{preset.label}</strong>
              <span className="preset-card__text">{preset.description}</span>
            </button>
          );
        })}
      </div>

      <div className="goals__layout">
        <GlassCard as="section" className="goals__sliders" aria-label="Metas por classe">
          {ASSET_CLASSES.map((assetClass) => {
            const id = `goal-${assetClass}`;
            const value = draft[assetClass];
            return (
              <div key={assetClass} className="goal-row" style={{ ["--class-color" as string]: classColor(assetClass) }}>
                <div className="goal-row__head">
                  <label htmlFor={id} className="goal-row__label">
                    <span className="dot" aria-hidden="true" />
                    {CLASS_META[assetClass].label}
                  </label>
                  <span className="goal-row__value">{formatPercent(value, 0)}</span>
                </div>
                <input
                  id={id}
                  className="range range--class"
                  type="range"
                  min={0}
                  max={100}
                  step={1}
                  value={value}
                  style={{ ["--fill" as string]: `${value}%` }}
                  onChange={(event) => change({ ...draft, [assetClass]: Number(event.target.value) })}
                />
                <span className="goal-row__now">hoje {formatPercent(current[assetClass] ?? 0)}</span>
              </div>
            );
          })}
        </GlassCard>

        <GlassCard as="aside" className="goals__summary" aria-label="Resumo das metas">
          <SumRing sum={sum} />
          <Donut
            size={150}
            thickness={14}
            label="Distribuição das metas"
            segments={ASSET_CLASSES.map((c) => ({
              key: c,
              value: draft[c],
              color: classColor(c),
              label: CLASS_META[c].label,
            }))}
          />
          <div className="goals__actions">
            <MagneticButton variant="ghost" magnetic={false} disabled={sum === 0 || complete} onClick={() => change(balanceTargets(draft))}>
              Equilibrar
            </MagneticButton>
            <MagneticButton disabled={!complete || !dirty} loading={save.isPending} onClick={submit}>
              Salvar
            </MagneticButton>
          </div>
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
        </GlassCard>
      </div>
    </div>
  );
}
