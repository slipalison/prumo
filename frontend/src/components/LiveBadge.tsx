import type { LiveStatus } from "../api/live";
import { formatRelative } from "../lib/format";

type LiveBadgeProps = {
  status: LiveStatus;
  /** De quando é o preço mais velho da carteira — mostrado fora do ao vivo. */
  updatedAt: string | null;
  /** A carteira tem ativo da B3, cujo preço gratuito chega 15 min atrasado. */
  delayedB3: boolean;
};

/**
 * De onde vem o número que a pessoa está vendo. Ao vivo, o ponto pulsa; fora
 * dele, a hora da última cotação. Muda de texto só quando muda o estado, e é
 * isso que o leitor de tela anuncia — preço novo a cada segundo, não.
 */
export function LiveBadge({ status, updatedAt, delayedB3 }: Readonly<LiveBadgeProps>) {
  return (
    <output className="live-badge">
      <LiveBadgeText status={status} updatedAt={updatedAt} delayedB3={delayedB3} />
    </output>
  );
}

/** O texto do selo em cada estado: ao vivo, conectando ou a hora da última cotação. */
function LiveBadgeText({ status, updatedAt, delayedB3 }: Readonly<LiveBadgeProps>) {
  if (status === "live") {
    return (
      <>
        <span className="live-badge__dot" aria-hidden="true" />
        <span className="live-badge__label">Ao vivo</span>
        {delayedB3 && <span className="live-badge__note">B3 com 15 min de atraso</span>}
      </>
    );
  }
  if (status === "connecting") return <span className="field__hint">Conectando ao vivo…</span>;
  return <span className="field__hint">Cotações {formatRelative(updatedAt)}</span>;
}
