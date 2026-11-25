import { QueryClientProvider, type QueryClient } from "@tanstack/react-query";
import { MotionConfig } from "motion/react";
import type { ReactNode } from "react";
import { ToastProvider } from "../components/ToastProvider";

type ProvidersProps = { client: QueryClient; children: ReactNode };

/** `reducedMotion="user"`: quem pede menos movimento no sistema recebe menos. */
export function Providers({ client, children }: ProvidersProps) {
  return (
    <QueryClientProvider client={client}>
      <MotionConfig reducedMotion="user">
        <ToastProvider>{children}</ToastProvider>
      </MotionConfig>
    </QueryClientProvider>
  );
}
