import { useSyncExternalStore } from "react";
import type { LiveStatus } from "../api/live";

// Um stream por aba: o estado dele mora fora do React, e quem quiser mostrar
// assina. Trocar de tela não reabre a conexão.
let status: LiveStatus = "off";
const listeners = new Set<() => void>();

export function setLiveStatus(next: LiveStatus): void {
  if (next === status) return;
  status = next;
  for (const listener of listeners) listener();
}

export function getLiveStatus(): LiveStatus {
  return status;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useLiveStatus(): LiveStatus {
  return useSyncExternalStore(subscribe, getLiveStatus, getLiveStatus);
}
