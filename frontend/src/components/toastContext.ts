import { createContext, useContext } from "react";

export type ToastTone = "success" | "error" | "info";
export type ToastInput = { tone: ToastTone; message: string };

export const ToastContext = createContext<(toast: ToastInput) => void>(() => undefined);

export function useToast(): (toast: ToastInput) => void {
  return useContext(ToastContext);
}
