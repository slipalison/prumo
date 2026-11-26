// Espelho do JSON de docs/api.md § Logos dos ativos.

export type IconEntry = {
  symbol: string;
  /** A versão do logo; vai na URL como `?v=` e muda quando o logo muda. */
  version: string;
};

export type IconIndex = { icons: IconEntry[] };
