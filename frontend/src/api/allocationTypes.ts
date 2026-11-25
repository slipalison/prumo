// Espelho de `GET/PUT /api/v1/settings/allocation` (docs/api.md).

/** Os guarda-corpos do motor do aporte que a pessoa liga. */
export type AllocationSettings = {
  /**
   * Nenhum ativo passa desta fatia da carteira depois do aporte, de 1 a 100.
   * `null` desliga.
   */
  max_asset_percent: number | null;
};
