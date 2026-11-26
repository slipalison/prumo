// Espelho do JSON de `GET/PUT /income/auto` de docs/api.md (§ Proventos automáticos).

export type StockEventKind = "bonus" | "split" | "reverse_split";

/** Um evento em ações que a B3 anunciou e o livro ainda não tem. */
export type PendingStockEvent = {
  asset_id: string;
  symbol: string;
  kind: StockEventKind;
  /** `AAAA-MM-DD`: a data com do evento. */
  record_date: string;
  /**
   * Como a B3 publica: percentual na bonificação e no desdobramento (`2` é 2%,
   * `100` é cada ação virar duas); multiplicador no grupamento (`0.1` é dez
   * virarem uma).
   */
  factor: number;
  /** As cotas do livro no fim da data com. */
  quantity: number;
};

export type IncomeAuto = {
  enabled: boolean;
  /** A busca mais velha entre os emissores da carteira; `null` antes da primeira. */
  fetched_at: string | null;
  /** Ativos da B3 que a fonte cobre. */
  covered: string[];
  /** Ativos da B3 sem ISIN conhecido: ficam sem automático. */
  unmatched: string[];
  pending_events: PendingStockEvent[];
};
