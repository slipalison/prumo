import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef } from "react";
import { useInfo, useMe, usePortfolio } from "../api/queries";
import type { Portfolio } from "../api/types";
import { AuroraBackground } from "../components/AuroraBackground";
import { ErrorState } from "../components/ErrorState";
import { NavBar } from "../components/NavBar";
import { ScreenSkeleton } from "../components/Skeleton";
import { useRoute } from "../lib/route";
import { APP_ROUTES, ROUTE_IDS } from "./routes";
import { useLivePrices } from "./useLivePrices";

/**
 * Cada tela abre do topo, e não na rolagem da anterior. Sem animação de
 * rolagem: um salto seco não incomoda quem pede menos movimento, e a tela nova
 * já entra com o próprio fade.
 */
function scrollToTop(): void {
  window.scrollTo({ top: 0, left: 0, behavior: "instant" });
}

function Screen({ route, portfolio }: { route: (typeof APP_ROUTES)[number]; portfolio: Portfolio }) {
  const { Screen: Current } = route;
  return <Current portfolio={portfolio} />;
}

export function Shell() {
  const { id } = useRoute(ROUTE_IDS);
  const route = APP_ROUTES.find((entry) => entry.id === id) ?? APP_ROUTES[0];
  const portfolio = usePortfolio();
  const me = useMe();
  const info = useInfo();
  const mainRef = useRef<HTMLElement>(null);
  useLivePrices(portfolio.isSuccess);

  useEffect(() => {
    document.title = `${route.label} · Prumo`;
  }, [route]);

  return (
    <>
      <button type="button" className="skip-link" onClick={() => mainRef.current?.focus()}>
        Pular para o conteúdo
      </button>
      <AuroraBackground />
      <div className="layout">
        <NavBar route={route.id} email={me.data?.email} version={info.data?.version} />
        <main ref={mainRef} className="main" tabIndex={-1}>
          {portfolio.isPending && <ScreenSkeleton />}
          {portfolio.isError && <ErrorState error={portfolio.error} onRetry={() => void portfolio.refetch()} />}
          {portfolio.data && (
            <AnimatePresence mode="wait" initial={false} onExitComplete={scrollToTop}>
              <motion.div
                key={route.id}
                className="route"
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -8 }}
                transition={{ duration: 0.22, ease: "easeOut" }}
              >
                <Screen route={route} portfolio={portfolio.data} />
              </motion.div>
            </AnimatePresence>
          )}
        </main>
      </div>
    </>
  );
}
