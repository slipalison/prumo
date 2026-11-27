import type { Figure, FundMarket, Rates } from "../../api/fundamentalsTypes";
import { InfoTip } from "../../components/InfoTip";
import { formatPercent } from "../../lib/format";
import type { Money } from "../../lib/money";
import { priceToBookText, ratesText } from "./fundIndicatorsText";
import { growthSign } from "./fundamentalsText";
import { Missing } from "./Missing";
import "./fundIndicators.css";

/** A régua do P/VP vai de 0,5 a 1,5: o traço em 1 fica no meio. */
const GAUGE_MIN = 0.5;
const GAUGE_MAX = 1.5;

const decimals = (value: number) =>
  value.toLocaleString("pt-BR", { minimumFractionDigits: 2, maximumFractionDigits: 2 });

/**
 * O P/VP com a marca em 1: o número e uma régua de 0,5 a 1,5 com o traço em 1
 * e o ponto no valor (fora da régua, o ponto para na ponta e o número diz o
 * resto). Só posição, sem cor de bom ou ruim (docs/design.md).
 */
export function PriceToBook({ figure }: Readonly<{ figure: Figure }>) {
  if (figure.value === null) return <Missing gap={figure.gap} />;
  const value = figure.value;
  const clamped = Math.min(GAUGE_MAX, Math.max(GAUGE_MIN, value));
  const left = ((clamped - GAUGE_MIN) / (GAUGE_MAX - GAUGE_MIN)) * 100;
  return (
    <span className="pvp">
      <span className="pvp__value">{decimals(value)}</span>
      <span // NOSONAR: régua desenhada em CSS (traço e ponto), não há imagem para um <img>; o papel dá o nome ao conjunto.
        className="pvp__gauge"
        role="img"
        aria-label={`P/VP ${decimals(value)}: ${priceToBookText(value)}`}
      >
        <span className="pvp__one" aria-hidden="true" />
        <span className="pvp__dot" aria-hidden="true" style={{ left: `${left}%` }} />
      </span>
    </span>
  );
}

const percentOf = (figure: Figure) =>
  figure.value === null ? <Missing gap={figure.gap} /> : formatPercent(figure.value * 100);

/** Crescimento anual composto, com o sinal (+5,2% a.a.). */
function growthOf(figure: Figure) {
  if (figure.value === null) return <Missing gap={figure.gap} />;
  return `${growthSign(figure.value)}${formatPercent(Math.abs(figure.value) * 100)} a.a.`;
}

type FundMarketGridProps = {
  /** O caixa do informe da CVM. */
  cashShare: Figure;
  market: FundMarket;
  rates: Rates;
  money: Money;
  /** Com o P/VP (a linha aberta da Análise); na folha ele já está acima. */
  priceToBook?: Figure;
  /** Rótulo do ativo, para os ⓘ ("Sobre o rendimento de KNRI11"). */
  label: string;
};

/**
 * Os indicadores do FII que saem do caixa do informe e do histórico de
 * mercado: rendimento de 12 meses (com o CDI líquido e o juro real ao lado,
 * só contexto), caixa, liquidez média diária e o crescimento dos proventos e
 * da cota em 3 e 5 anos. Sem veredito: o número e o motivo quando falta.
 */
export function FundMarketGrid({ cashShare, market, rates, money, priceToBook, label }: Readonly<FundMarketGridProps>) {
  const context = ratesText(rates);
  const reais = (figure: Figure) => (figure.value === null ? <Missing gap={figure.gap} /> : money.compact(figure.value));
  return (
    <dl className="fund-indicators">
      {priceToBook && (
        <div>
          <dt>P/VP</dt>
          <dd>
            <PriceToBook figure={priceToBook} />
          </dd>
        </div>
      )}
      <div>
        <dt className="fund-indicators__term">
          Rendimento 12 meses
          <InfoTip label={`Sobre o rendimento de ${label}`} align="start">
            Proventos por cota com data ex nos últimos 12 meses sobre a cotação de hoje. O CDI de 12 meses depois do IR de
            15% e o juro real da NTN-B de referência ficam ao lado só como contexto: o rendimento do FII é isento para a
            pessoa física e não tem garantia.
          </InfoTip>
        </dt>
        <dd>
          {percentOf(market.dividend_yield_12m)}
          {context && <span className="fund-indicators__context">{context}</span>}
        </dd>
      </div>
      <div>
        <dt>Caixa</dt>
        <dd>
          {cashShare.value === null ? <Missing gap={cashShare.gap} /> : `${formatPercent(cashShare.value * 100)} do ativo`}
        </dd>
      </div>
      <div>
        <dt>Liquidez média diária</dt>
        <dd>
          {reais(market.liquidity_30d)}
          <span className="fund-indicators__context">
            em 30 pregões; {reais(market.liquidity_90d)} em 90
          </span>
        </dd>
      </div>
      <div>
        <dt>Proventos por cota, CAGR</dt>
        <dd>
          {growthOf(market.dividend_cagr_3y)} <span className="fund-indicators__span">3 anos</span>
          <span className="fund-indicators__context">
            {growthOf(market.dividend_cagr_5y)} em 5 anos
          </span>
        </dd>
      </div>
      <div>
        <dt>Cota, CAGR</dt>
        <dd>
          {growthOf(market.price_cagr_3y)} <span className="fund-indicators__span">3 anos</span>
          <span className="fund-indicators__context">
            {growthOf(market.price_cagr_5y)} em 5 anos
          </span>
        </dd>
      </div>
    </dl>
  );
}
