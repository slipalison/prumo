import type { HeatCell, HeatYear } from "../../api/incomeHistoryTypes";
import { cx } from "../../lib/cx";
import {
  HEAT_LEVELS,
  HEAT_MONTHS,
  heatDetail,
  heatLevel,
  heatMax,
  heatPartial,
  heatText,
  heatValue,
  monthName,
  type HeatMetric,
} from "../../lib/incomeHistory";
import { useMoney } from "../../lib/privacy";

const METRIC_NAME: Record<HeatMetric, string> = {
  amount: "Proventos em reais",
  dy: "DY (provento do mês ÷ patrimônio médio do mês)",
  yoc: "Yield on cost (provento do mês ÷ custo médio do mês)",
};

type IncomeHeatmapProps = {
  years: HeatYear[];
  metric: HeatMetric;
  /** O que a escolha de cima diz (pela data de pagamento, líquido...), para a legenda da tabela. */
  context: string;
};

/**
 * Mês × ano: cada célula traz o número (R$, DY ou yield on cost) e um fundo
 * dourado mais forte quanto maior ele é frente ao maior mês — a cor só
 * reforça o número escrito. É uma `<table>` de verdade: o leitor de tela anda
 * por ano e mês e ouve o valor exato e a conta. Na largura de celular cada ano
 * vira um bloco de quatro colunas de meses, sem rolagem lateral.
 */
export function IncomeHeatmap({ years, metric, context }: Readonly<IncomeHeatmapProps>) {
  const money = useMoney();
  const max = heatMax(years, metric);
  const totalHeader = metric === "amount" ? "Total" : "No ano";

  /** O total do ano não entra na escala de cor: ele é a soma, não um mês. */
  const cell = (value: HeatCell, short: string, label: string, total = false) => {
    const level = total ? 0 : heatLevel(heatValue(value, metric), max);
    const partial = value.amount > 0 && heatPartial(value, metric);
    const detail = heatDetail(value, metric, money);
    const upcoming = value.upcoming > 0;
    return (
      <td
        key={short}
        data-label={short}
        className={cx("heatmap__cell", `heat-${level}`, upcoming && "is-upcoming", total && "heatmap__total")}
        title={`${label}: ${detail}`}
      >
        <span aria-hidden="true">
          {heatText(value, metric, money)}
          {partial && <span className="heatmap__mark">*</span>}
        </span>
        <span className="sr-only">{detail}</span>
      </td>
    );
  };

  return (
    <div className="heatmap">
      <table className="heatmap__table">
        <caption className="sr-only">{`${METRIC_NAME[metric]} por mês e ano, ${context}`}</caption>
        <thead>
          <tr>
            <th scope="col">Ano</th>
            {HEAT_MONTHS.map((month) => (
              <th key={month} scope="col">
                {month}
              </th>
            ))}
            <th scope="col">{totalHeader}</th>
          </tr>
        </thead>
        <tbody>
          {years.map((year) => (
            <tr key={year.year}>
              <th scope="row">{year.year}</th>
              {year.months.map((month, index) =>
                cell(month, HEAT_MONTHS[index] ?? String(index), monthName(year.year, index)),
              )}
              {cell(year.total, totalHeader, `${totalHeader} de ${year.year}`, true)}
            </tr>
          ))}
        </tbody>
      </table>
      <div className="heatmap__key">
        <span className="heatmap__scale" aria-hidden="true">
          menos
          {Array.from({ length: HEAT_LEVELS }, (_, index) => (
            <span key={index} className={`heatmap__swatch heat-${index + 1}`} />
          ))}
          mais
        </span>
        <span>
          <span className="heatmap__swatch heatmap__swatch--upcoming" aria-hidden="true" /> inclui a receber
        </span>
        {metric !== "amount" && (
          <span>
            <span aria-hidden="true">*</span> parcial: parte do provento fica fora da conta
          </span>
        )}
      </div>
    </div>
  );
}
