import type { ComponentType } from "react";
import type { Portfolio } from "../../api/types";
import { CriteriaScreen } from "../criteria/CriteriaScreen";
import { GoalsScreen } from "../goals/GoalsScreen";
import { HealthScreen } from "../health/HealthScreen";
import { ImportB3Screen } from "../importB3/ImportB3Screen";
import { ObjectivesScreen } from "../objectives/ObjectivesScreen";
import { TaxScreen } from "../tax/TaxScreen";

export type SettingsSection = {
  /** O trecho do endereço: `#/ajustes/<id>`. */
  id: string;
  /** O nome na lista de seções de Ajustes. */
  label: string;
  Component: ComponentType<{ portfolio: Portfolio }>;
};

/**
 * As seções de Ajustes, na ordem em que aparecem. Seção nova é uma linha aqui:
 * a lista, o endereço e o foco saem dela.
 */
export const SETTINGS_SECTIONS: readonly SettingsSection[] = [
  { id: "metas", label: "Metas por classe", Component: GoalsScreen },
  { id: "criterios", label: "Critérios", Component: CriteriaScreen },
  { id: "objetivos", label: "Objetivos", Component: ObjectivesScreen },
  { id: "saude", label: "Saúde da carteira", Component: HealthScreen },
  { id: "importar-b3", label: "Importar da B3", Component: ImportB3Screen },
  { id: "impostos", label: "Impostos", Component: TaxScreen },
];
