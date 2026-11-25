// Espelho do JSON de `/alerts` (docs/api.md § Alertas).

export const ALERT_KINDS = [
  "income_tomorrow",
  "class_drift",
  "price_below_ceiling",
  "objective_achieved",
  "tax_exemption",
  "health_warning",
] as const;
export type AlertKind = (typeof ALERT_KINDS)[number];

/** Em que o limite de um tipo é medido. */
export type AlertUnit = "percentage_points" | "percent_of_limit";

export type AlertType = {
  kind: AlertKind;
  enabled: boolean;
  /** `false`: o servidor ainda não apura este tipo. */
  available: boolean;
  threshold: number | null;
  default_threshold: number | null;
  unit: AlertUnit | null;
};

export type Alerts = {
  telegram: {
    /** O servidor tem o token do bot. */
    configured: boolean;
    /** Com quem conversar, sem o `@`. */
    bot_username: string | null;
    linked: boolean;
    linked_at: string | null;
  };
  /** `HH:MM`, horário de Brasília. */
  send_at: string;
  hide_values: boolean;
  types: AlertType[];
};

export type TypeChange = { enabled?: boolean; threshold?: number };

export type AlertsRequest = {
  send_at?: string;
  hide_values?: boolean;
  types?: Partial<Record<AlertKind, TypeChange>>;
};

export type LinkCode = {
  code: string;
  /** O que a pessoa manda ao bot: `/start CODIGO`. */
  command: string;
  bot_username: string | null;
  expires_at: string;
};
