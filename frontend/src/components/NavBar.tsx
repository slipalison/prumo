import { motion } from "motion/react";
import { APP_ROUTES, type RouteId } from "../app/routes";
import { cx } from "../lib/cx";
import { Logo } from "./Logo";
import { PrivacyToggle } from "./PrivacyToggle";
import { ThemeToggle } from "./ThemeToggle";

type NavBarProps = {
  route: RouteId;
  email?: string | undefined;
  version?: string | undefined;
};

/**
 * Uma navegação só: barra de abas embaixo no celular, trilho à esquerda a
 * partir de 960px (a troca é só CSS). O destaque da aba escorrega. As entradas
 * saem de `APP_ROUTES`. O olho dos valores fica junto do tema, no canto.
 */
export function NavBar({ route, email, version }: NavBarProps) {
  return (
    <nav className="nav" aria-label="Principal">
      <div className="nav__brand">
        <Logo />
      </div>
      <ul className="nav__list">
        {APP_ROUTES.map(({ id, label, Icon }) => {
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
                <span className="nav__label">{label}</span>
              </a>
            </li>
          );
        })}
      </ul>
      <div className="nav__footer">
        <PrivacyToggle />
        <ThemeToggle />
        {email && <span className="nav__meta">{email}</span>}
        {version && <span className="nav__meta">v{version}</span>}
      </div>
    </nav>
  );
}
