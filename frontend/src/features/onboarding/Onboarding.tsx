import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState } from "react";
import { errorMessage } from "../../api/client";
import { useSaveGoals } from "../../api/queries";
import type { Asset, Portfolio } from "../../api/types";
import { MagneticButton } from "../../components/MagneticButton";
import { Plumb } from "../../components/Plumb";
import { Sheet } from "../../components/Sheet";
import { celebrate } from "../../lib/celebrate";
import { cx } from "../../lib/cx";
import { isComplete, PRESETS, sameTargets, targetsFromClasses, type PresetId } from "../../lib/goals";
import { assetName, needsRating } from "../../lib/rating";
import { AssetForm } from "../portfolio/AssetForm";

type OnboardingProps = {
  open: boolean;
  portfolio: Portfolio;
  onClose: () => void;
  /** "Dar nota": fecha o assistente e abre a folha do ativo no checklist. */
  onRateAsset: (asset: Asset) => void;
};

const STEPS = ["Perfil", "Primeiro ativo", "Pronto"] as const;

/**
 * Três passos e a carteira existe: perfil (que vira as metas), o primeiro
 * ativo, e o convite para calcular o primeiro aporte.
 */
export function Onboarding({ open, portfolio, onClose, onRateAsset }: OnboardingProps) {
  const [step, setStep] = useState(0);
  const [created, setCreated] = useState<Asset | null>(null);

  useEffect(() => {
    if (open) {
      setStep(0);
      setCreated(null);
    }
  }, [open]);

  return (
    <Sheet open={open} onClose={onClose} title="Montar minha carteira" className="onboarding">
      <ol className="steps" aria-label="Etapas">
        {STEPS.map((label, index) => (
          <li
            key={label}
            className={cx("steps__item", index === step && "is-current", index < step && "is-done")}
            aria-current={index === step ? "step" : undefined}
          >
            <span className="steps__dot" aria-hidden="true" />
            <span className="steps__label">{label}</span>
          </li>
        ))}
      </ol>
      <AnimatePresence mode="wait" initial={false}>
        <motion.div
          key={step}
          initial={{ opacity: 0, x: 24 }}
          animate={{ opacity: 1, x: 0 }}
          exit={{ opacity: 0, x: -24 }}
          transition={{ duration: 0.22 }}
        >
          {step === 0 && <ProfileStep portfolio={portfolio} onNext={() => setStep(1)} />}
          {step === 1 && (
            <AssetStep
              onNext={(asset) => {
                setCreated(asset);
                setStep(2);
              }}
            />
          )}
          {step === 2 && <DoneStep asset={created} onClose={onClose} onRate={onRateAsset} />}
        </motion.div>
      </AnimatePresence>
    </Sheet>
  );
}

function ProfileStep({ portfolio, onNext }: { portfolio: Portfolio; onNext: () => void }) {
  const current = targetsFromClasses(portfolio.classes);
  const initial = PRESETS.find((p) => sameTargets(p.targets, current))?.id ?? null;
  const [selected, setSelected] = useState<PresetId | null>(initial);
  const save = useSaveGoals();
  const hasGoals = isComplete(current);

  const next = () => {
    const preset = PRESETS.find((p) => p.id === selected);
    if (!preset) {
      onNext();
      return;
    }
    if (sameTargets(preset.targets, current)) {
      onNext();
      return;
    }
    save.mutate(preset.targets, { onSuccess: onNext });
  };

  return (
    <div className="onboarding__step">
      <h3 className="onboarding__title">Qual é o seu jeito de investir?</h3>
      <p className="field__hint">Vira suas metas por classe. Dá para ajustar depois, em Metas.</p>
      <div className="preset-grid" role="radiogroup" aria-label="Perfil">
        {PRESETS.map((preset) => (
          <button
            key={preset.id}
            type="button"
            role="radio"
            aria-checked={selected === preset.id}
            className={cx("preset-card glass", selected === preset.id && "is-active")}
            onClick={() => setSelected(preset.id)}
          >
            <strong className="preset-card__title">{preset.label}</strong>
            <span className="preset-card__text">{preset.description}</span>
          </button>
        ))}
      </div>
      {save.isError && (
        <p className="form-error" role="alert">
          {errorMessage(save.error)}
        </p>
      )}
      <MagneticButton onClick={next} disabled={!selected && !hasGoals} loading={save.isPending} className="wide">
        {selected || hasGoals ? "Continuar" : "Escolha um perfil"}
      </MagneticButton>
    </div>
  );
}

function AssetStep({ onNext }: { onNext: (asset: Asset) => void }) {
  return (
    <div className="onboarding__step">
      <h3 className="onboarding__title">Qual ativo você já tem?</h3>
      <p className="field__hint">Comece por um. Os outros você adiciona na Carteira.</p>
      <AssetForm submitLabel="Adicionar e continuar" onCreated={onNext} />
    </div>
  );
}

type DoneStepProps = { asset: Asset | null; onClose: () => void; onRate: (asset: Asset) => void };

function DoneStep({ asset, onClose, onRate }: DoneStepProps) {
  useEffect(() => {
    celebrate();
  }, []);

  // Ativo de Ações/FIIs nasce sem nota: sem ela, o primeiro cálculo daria
  // "nada a comprar". A ação principal passa a ser dar a nota.
  if (asset && needsRating(asset)) {
    const name = assetName(asset);
    return (
      <div className="onboarding__step onboarding__done">
        <Plumb state="settled" size={84} label="Quase no eixo" />
        <h3 className="onboarding__title">Falta só a nota do {name}</h3>
        <p className="field__hint">
          Sem nota ele não recebe aporte. Responda o checklist — leva um minuto, e cada sim soma 1.
        </p>
        <MagneticButton onClick={() => onRate(asset)} className="wide">
          Dar nota ao {name}
        </MagneticButton>
        <MagneticButton variant="ghost" magnetic={false} onClick={onClose} className="wide">
          Calcular mesmo assim
        </MagneticButton>
      </div>
    );
  }

  return (
    <div className="onboarding__step onboarding__done">
      <Plumb state="success" size={84} label="Carteira no eixo" />
      <h3 className="onboarding__title">Sua carteira está no prumo</h3>
      <p className="field__hint">Agora é só dizer quanto vai aportar.</p>
      <MagneticButton onClick={onClose} className="wide">
        Calcular meu primeiro aporte
      </MagneticButton>
    </div>
  );
}
