// Espelho do JSON de `GET /cash` (docs/api.md § Caixa).

export type CashMovementKind = "income" | "sale" | "contribution";

export type CashMovement = {
  /** `AAAA-MM-DD`. */
  date: string;
  kind: CashMovementKind;
  /** O ativo (símbolo ou nome) ou "Aporte". */
  label: string;
  /** Sempre positivo; `kind` diz se entrou (`income`, `sale`) ou saiu (`contribution`). */
  amount: number;
};

export type Cash = {
  /** O que há para aportar. Nunca negativo. */
  balance: number;
  /** O dia em que o caixa começa a contar (`AAAA-MM-DD`). */
  since: string;
  /** Proventos líquidos recebidos desde `since`. */
  income: { amount: number; count: number };
  /** Líquido das vendas e resgates do livro desde `since`. */
  sales: { amount: number; count: number };
  /** O que os aportes já tiraram do caixa. */
  used: number;
  /** Saída que passou do saldo e foi cortada em zero (normalmente 0). */
  adjustment: number;
  /** Os movimentos mais novos, do mais novo para o mais velho. */
  recent: CashMovement[];
};
