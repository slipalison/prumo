import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { openPriceStream } from "../api/live";
import { keys } from "../api/queries";
import type { Portfolio } from "../api/types";
import { mergePrices } from "../lib/livePrices";
import { setLiveStatus } from "../lib/liveStatus";

/**
 * Mantém a carteira em dia sem recarregar: cada evento do stream vai direto
 * para o cache do `GET /portfolio`, e toda tela que lê a carteira acompanha.
 *
 * Aba escondida fecha a conexão (bateria e dados no celular); ao voltar, abre
 * de novo e o primeiro evento já traz o preço de agora.
 */
export function useLivePrices(enabled: boolean): void {
  const client = useQueryClient();

  useEffect(() => {
    if (!enabled) return undefined;
    let close: (() => void) | null = null;

    const start = () => {
      close ??= openPriceStream({
        onPrices: (prices) =>
          client.setQueryData<Portfolio>(keys.portfolio, (current) => current && mergePrices(current, prices)),
        onStatus: setLiveStatus,
      });
    };
    const stop = () => {
      close?.();
      close = null;
    };
    const onVisibility = () => (document.visibilityState === "hidden" ? stop() : start());

    if (document.visibilityState !== "hidden") start();
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
      stop();
    };
  }, [enabled, client]);
}
