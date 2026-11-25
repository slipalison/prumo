import { errorMessage } from "../api/client";
import { GlassCard } from "./GlassCard";
import { MagneticButton } from "./MagneticButton";
import { Plumb } from "./Plumb";

type ErrorStateProps = { error: unknown; onRetry: () => void };

export function ErrorState({ error, onRetry }: ErrorStateProps) {
  return (
    <GlassCard as="section" className="state-card" role="alert">
      <Plumb state="idle" size={64} label="Prumo fora do eixo" />
      <h1 className="state-card__title">Não consegui carregar sua carteira</h1>
      <p className="state-card__text">{errorMessage(error)}</p>
      <MagneticButton onClick={onRetry}>Tentar de novo</MagneticButton>
    </GlassCard>
  );
}
