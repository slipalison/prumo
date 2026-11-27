import { motion } from "motion/react";
import { useEffect, useRef } from "react";
import type { Portfolio } from "../../api/types";
import { Page } from "../../components/Layout";
import { cx } from "../../lib/cx";
import { enterUp } from "../../lib/motion";
import { parseSection, useHash } from "../../lib/route";
import { SETTINGS_SECTIONS } from "./sections";

type SettingsScreenProps = { portfolio: Portfolio };

/**
 * Ajustes: o que a pessoa configura uma vez e raramente revê. Cada seção é uma
 * tela própria (`#/ajustes/<seção>`); sem seção no endereço, abre a primeira.
 *
 * Com espaço (≥ 1000 px de conteúdo), a lista das seções é uma coluna à
 * esquerda, fixa ao rolar, e a seção ocupa o resto; mais estreito, as seções
 * viram pílulas numa linha só, que rola de lado. A casca é uma `<Page>`: a
 * largura larga vale para Ajustes inteiro, e cada seção enxerga só a sua
 * largura (a seção é o contêiner das grades dela).
 */
export function SettingsScreen({ portfolio }: Readonly<SettingsScreenProps>) {
  const requested = parseSection(useHash());
  const section = SETTINGS_SECTIONS.find((s) => s.id === requested) ?? SETTINGS_SECTIONS[0];
  const list = useRef<HTMLUListElement>(null);

  // Nas pílulas que rolam de lado (celular), a seção aberta fica à vista.
  useEffect(() => {
    const row = list.current;
    const active = row?.querySelector<HTMLElement>("[aria-current='page']");
    if (!row || !active || row.scrollWidth <= row.clientWidth) return;
    row.scrollLeft = Math.max(active.offsetLeft - row.offsetLeft - 16, 0);
  }, [section?.id]);

  if (!section) return null;
  const { Component } = section;

  return (
    <Page className="settings">
      <div className="settings__layout">
        <nav className="settings__nav" aria-label="Seções de Ajustes">
          <span className="eyebrow settings__title">Ajustes</span>
          <ul ref={list} className="settings__list">
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
          className="settings__section"
          initial={enterUp.initial}
          animate={enterUp.animate}
          transition={enterUp.transition}
        >
          <Component portfolio={portfolio} />
        </motion.div>
      </div>
    </Page>
  );
}
