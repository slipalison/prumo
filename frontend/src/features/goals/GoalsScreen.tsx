import { useEffect, useMemo, useState } from "react";
import { errorMessage } from "../../api/client";
import { useSaveGoals } from "../../api/queries";
import type { AssetClass, GoalTargets, Portfolio } from "../../api/types";
import { Donut } from "../../components/Donut";
import { InfoTip } from "../../components/InfoTip";
import { Grid, Page, PageHeader, Panel } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { useToast } from "../../components/toastContext";
import { ASSET_CLASSES, CLASS_META, classColor } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { formatPercent } from "../../lib/format";
import { balanceTargets, isComplete, PRESETS, sameTargets, sumTargets, targetsFromClasses } from "../../lib/goals";
import { ConcentrationLimitCard } from "../allocation/ConcentrationLimitCard";
import { SumRing } from "./SumRing";

type GoalsScreenProps = { portfolio: Portfolio };

export function GoalsScreen({ portfolio }: Readonly<GoalsScreenProps>) {
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
    <Page className="goals">
      <PageHeader
        title="Quanto você quer em cada classe?"
        infoLabel="Como as metas guiam o aporte"
        info="Os aportes vão sempre para o que está mais longe da meta."
        actions={
          <div className="preset-row">
            <fieldset className="preset-row__group" aria-label="Perfis prontos">
              {PRESETS.map((preset) => (
                <button
                  key={preset.id}
                  type="button"
                  aria-pressed={sameTargets(preset.targets, draft)}
                  className={cx("preset-btn", sameTargets(preset.targets, draft) && "is-active")}
                  onClick={() => change(preset.targets)}
                >
                  {preset.label}
                </button>
              ))}
            </fieldset>
            <InfoTip label="O que cada perfil pronto faz" align="end" placement="bottom">
              <span className="preset-row__tip">
                {PRESETS.map((preset) => (
                  <span key={preset.id}>
                    <strong>{preset.label}:</strong> {preset.description}
                  </span>
                ))}
              </span>
            </InfoTip>
          </div>
        }
      />

      <Grid cols="main-aside" alignStart>
        <Panel title="Metas por classe">
          <Grid cols={2} className="goals__sliders">
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
          </Grid>
        </Panel>

        <div className="goals__side">
          <Panel as="aside" title="Resumo das metas" className="goals__summary">
            <div className="goals__figures">
              <SumRing sum={sum} />
              <Donut
                size={120}
                thickness={12}
                label="Distribuição das metas"
                segments={ASSET_CLASSES.map((c) => ({
                  key: c,
                  value: draft[c],
                  color: classColor(c),
                  label: CLASS_META[c].label,
                }))}
              />
            </div>
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
          </Panel>

          <ConcentrationLimitCard />
        </div>
      </Grid>
    </Page>
  );
}
