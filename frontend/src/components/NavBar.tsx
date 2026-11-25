import { motion } from "motion/react";
import type { ComponentType, SVGProps } from "react";
import { cx } from "../lib/cx";
import { ROUTES, ROUTE_LABEL, type RouteId } from "../lib/route";
import { IconChecklist, IconCoins, IconTarget, IconWallet } from "./icons";
import { Logo } from "./Logo";
import { ThemeToggle } from "./ThemeToggle";

const ICON: Record<RouteId, ComponentType<SVGProps<SVGSVGElement>>> = {
  aportar: IconCoins,
  carteira: IconWallet,
  metas: IconTarget,
  criterios: IconChecklist,
};

type NavBarProps = {
  route: RouteId;
  email?: string | undefined;
  version?: string | undefined;
};

/**
 * Uma navegação só: barra de abas embaixo no celular, trilho à esquerda a
 * partir de 960px (a troca é só CSS). O destaque da aba escorrega.
 */
export function NavBar({ route, email, version }: NavBarProps) {
  return (
    <nav className="nav" aria-label="Principal">
      <div className="nav__brand">
        <Logo />
      </div>
      <ul className="nav__list">
        {ROUTES.map((id) => {
          const Icon = ICON[id];
          const active = id === route;
          return (
            <li key={id} className="nav__entry">
              <a
                href={`#/${id}`}
                className={cx("nav__item", active && "is-active")}
                aria-current={active ? "page" : undefined}
              >
                {active && (
                  <motion.span
                    layoutId="nav-indicator"
                    className="nav__indicator"
                    transition={{ type: "spring", stiffness: 420, damping: 34 }}
                  />
                )}
                <Icon className="nav__icon" />
                <span className="nav__label">{ROUTE_LABEL[id]}</span>
              </a>
            </li>
          );
        })}
      </ul>
      <div className="nav__footer">
        <ThemeToggle />
        {email && <span className="nav__meta">{email}</span>}
        {version && <span className="nav__meta">v{version}</span>}
      </div>
    </nav>
  );
}
