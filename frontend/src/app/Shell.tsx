import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef } from "react";
import { useInfo, useMe, usePortfolio } from "../api/queries";
import type { Portfolio } from "../api/types";
import { AuroraBackground } from "../components/AuroraBackground";
import { ErrorState } from "../components/ErrorState";
import { NavBar } from "../components/NavBar";
import { ScreenSkeleton } from "../components/Skeleton";
import { ContributeScreen } from "../features/contribute/ContributeScreen";
import { CriteriaScreen } from "../features/criteria/CriteriaScreen";
import { GoalsScreen } from "../features/goals/GoalsScreen";
import { PortfolioScreen } from "../features/portfolio/PortfolioScreen";
import { ROUTE_LABEL, useRoute, type RouteId } from "../lib/route";
import { useLivePrices } from "./useLivePrices";

/**
 * Cada tela abre do topo, e não na rolagem da anterior. Sem animação de
 * rolagem: um salto seco não incomoda quem pede menos movimento, e a tela nova
 * já entra com o próprio fade.
 */
function scrollToTop(): void {
  window.scrollTo({ top: 0, left: 0, behavior: "instant" });
}

function Screen({ route, portfolio }: { route: RouteId; portfolio: Portfolio }) {
  switch (route) {
    case "carteira":
      return <PortfolioScreen portfolio={portfolio} />;
    case "metas":
      return <GoalsScreen portfolio={portfolio} />;
    case "criterios":
      return <CriteriaScreen portfolio={portfolio} />;
    default:
      return <ContributeScreen portfolio={portfolio} />;
  }
}

export function Shell() {
  const [route] = useRoute();
  const portfolio = usePortfolio();
  const me = useMe();
  const info = useInfo();
  const mainRef = useRef<HTMLElement>(null);
  useLivePrices(portfolio.isSuccess);

  useEffect(() => {
    document.title = `${ROUTE_LABEL[route]} · Prumo`;
  }, [route]);

  return (
    <>
      <button type="button" className="skip-link" onClick={() => mainRef.current?.focus()}>
        Pular para o conteúdo
      </button>
      <AuroraBackground />
      <div className="layout">
        <NavBar route={route} email={me.data?.email} version={info.data?.version} />
        <main ref={mainRef} className="main" tabIndex={-1}>
          {portfolio.isPending && <ScreenSkeleton />}
          {portfolio.isError && <ErrorState error={portfolio.error} onRetry={() => void portfolio.refetch()} />}
          {portfolio.data && (
            <AnimatePresence mode="wait" initial={false} onExitComplete={scrollToTop}>
              <motion.div
                key={route}
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
