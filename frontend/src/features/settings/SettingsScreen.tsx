import { motion } from "motion/react";
import type { Portfolio } from "../../api/types";
import { cx } from "../../lib/cx";
import { parseSection, useHash } from "../../lib/route";
import { SETTINGS_SECTIONS } from "./sections";

type SettingsScreenProps = { portfolio: Portfolio };

/**
 * Ajustes: o que a pessoa configura uma vez e raramente revê. Cada seção é uma
 * tela própria (`#/ajustes/<seção>`), com a lista de seções no topo; sem seção
 * no endereço, abre a primeira.
 */
export function SettingsScreen({ portfolio }: SettingsScreenProps) {
  const requested = parseSection(useHash());
  const section = SETTINGS_SECTIONS.find((s) => s.id === requested) ?? SETTINGS_SECTIONS[0];
  if (!section) return null;
  const { Component } = section;

  return (
    <div className="settings">
      <nav className="settings__nav" aria-label="Seções de Ajustes">
        <span className="eyebrow settings__title">Ajustes</span>
        <ul className="settings__list">
          {SETTINGS_SECTIONS.map((entry) => {
            const active = entry.id === section.id;
            return (
              <li key={entry.id}>
                <a
                  href={`#/ajustes/${entry.id}`}
                  className={cx("settings__link", active && "is-active")}
                  aria-current={active ? "page" : undefined}
                >
                  {entry.label}
                </a>
              </li>
            );
          })}
        </ul>
      </nav>
      <motion.div
        key={section.id}
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.2, ease: "easeOut" }}
      >
        <Component portfolio={portfolio} />
      </motion.div>
    </div>
  );
}
