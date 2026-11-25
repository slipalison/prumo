import type { ComponentType, SVGProps } from "react";
import type { Portfolio } from "../api/types";
import { IconChart, IconCoins, IconSliders, IconWallet } from "../components/icons";
import { ContributeScreen } from "../features/contribute/ContributeScreen";
import { PerformanceScreen } from "../features/performance/PerformanceScreen";
import { PortfolioScreen } from "../features/portfolio/PortfolioScreen";
import { SettingsScreen } from "../features/settings/SettingsScreen";

export type AppRoute = {
  /** O primeiro trecho do endereço: `#/<id>`. */
  id: string;
  /** O nome na navegação e no título da aba. */
  label: string;
  Icon: ComponentType<SVGProps<SVGSVGElement>>;
  Screen: ComponentType<{ portfolio: Portfolio }>;
};

/**
 * As telas do app, na ordem da navegação; a primeira é a abertura. Tela nova
 * é uma linha aqui — a barra, o título, o `aria-current` e o roteamento saem
 * desta lista.
 */
export const APP_ROUTES = [
  { id: "aportar", label: "Aportar", Icon: IconCoins, Screen: ContributeScreen },
  { id: "carteira", label: "Carteira", Icon: IconWallet, Screen: PortfolioScreen },
  { id: "desempenho", label: "Desempenho", Icon: IconChart, Screen: PerformanceScreen },
  { id: "ajustes", label: "Ajustes", Icon: IconSliders, Screen: SettingsScreen },
] as const satisfies readonly AppRoute[];

export type RouteId = (typeof APP_ROUTES)[number]["id"];

export const ROUTE_IDS: readonly RouteId[] = APP_ROUTES.map((route) => route.id);
