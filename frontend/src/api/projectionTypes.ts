// Espelho do JSON de `GET /income/projection` (docs/api.md § Renda projetada;
// método em docs/projecao.md).

/** O tipo do ativo na projeção. */
export type ProjectionGroup = "paper_reits" | "brick_reits" | "other_reits" | "br_stocks" | "foreign";

/** O que move a renda do tipo. */
export type ProjectionDriver = "cdi" | "ipca" | "flat";

/** De onde veio cada lado do caminho de juro e inflação. */
export type PathSource = "focus" | "informed" | "measured" | "fallback" | "missing";

export type ScenarioTotals = { low: number; base: number; high: number };

export type ProjectedMonth = ScenarioTotals & {
  /** `AAAA-MM`. */
  month: string;
  /** O base de cada tipo. */
  by_group: Partial<Record<ProjectionGroup, number>>;
};

export type ProjectedGroup = {
  group: ProjectionGroup;
  driver: ProjectionDriver;
  assets: number;
  /** A renda de partida do tipo, por mês. */
  monthly_start: number;
  totals: ScenarioTotals;
};

export type ProjectedAsset = {
  asset_id: string;
  symbol: string | null;
  display_name: string;
  group: ProjectionGroup;
  driver: ProjectionDriver;
  /** Os 12 meses da Renda ou a média do habitual. */
  basis: "trailing_12m" | "usual";
  usual_years: number | null;
  /** A partida da projeção, por mês. */
  monthly_start: number;
  /** Os 12 meses da estimativa da Renda, por mês (difere da partida na média do habitual). */
  estimate_monthly: number;
  totals: ScenarioTotals;
};

/** `estimate_monthly = start_monthly − usual_difference + unprojected_monthly`. */
export type ProjectionReconciliation = {
  start_monthly: number;
  /** Média do habitual menos os 12 meses (negativa: os 12 meses vieram acima do habitual). */
  usual_difference: number;
  /** A estimativa dos ativos sem tipo de renda, fora da projeção. */
  unprojected_monthly: number;
  usual_years: number | null;
};

export type ProjectionPremises = {
  horizon_months: 12 | 24;
  scenario_spread_percent: number;
  spread_source: "informed" | "default";
  cdi: {
    source: PathSource;
    now_percent: number | null;
    /** O CDI médio dos 12 meses que passaram: a base do FII de papel. */
    trailing_percent: number | null;
    end_percent: number | null;
  };
  ipca: { source: PathSource; annual_percent: number };
  focus: { survey_date: string; fetched_at: string | null } | null;
};

export type IncomeProjection = {
  today: string;
  /** A estimativa mensal da Renda, de onde a projeção parte. */
  estimate_monthly: number;
  reconciliation: ProjectionReconciliation;
  months: ProjectedMonth[];
  totals: ScenarioTotals;
  groups: ProjectedGroup[];
  assets: ProjectedAsset[];
  premises: ProjectionPremises;
  missing: string[];
};

/** O que a pessoa troca na consulta; ausente = o padrão. Taxas em % ao ano. */
export type ProjectionQuery = {
  months: 12 | 24;
  spread?: number;
  cdi?: number;
  ipca?: number;
};
