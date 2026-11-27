import type { IncomeYear } from "../../api/incomeTypes";
import { MONTH_HEADERS } from "../../lib/income";
import { useMoney } from "../../lib/privacy";

/**
 * Ano × mês: o líquido (recebido e a receber) de cada mês de cada ano. Estreita
 * (celular), cada ano vira um bloco com os meses em grade e o nome do mês antes
 * do valor (`data-label`), sem rolagem lateral.
 */
export function YearTable({ years }: { years: IncomeYear[] }) {
  const money = useMoney();
  return (
    <div className="income-table-wrap" role="region" aria-label="Renda por ano e mês">
      <table className="income-table">
        <caption className="sr-only">Renda líquida por ano e por mês</caption>
        <thead>
          <tr>
            <th scope="col">Ano</th>
            {MONTH_HEADERS.map((month) => (
              <th key={month} scope="col">
                {month}
              </th>
            ))}
            <th scope="col">Total</th>
          </tr>
        </thead>
        <tbody>
          {years.map((row) => (
            <tr key={row.year}>
              <th scope="row">{row.year}</th>
              {row.months.map((value, index) => (
                <td key={MONTH_HEADERS[index]} data-label={MONTH_HEADERS[index]} className={value === 0 ? "is-empty" : undefined}>
                  {value === 0 ? "–" : money.brl(value)}
                </td>
              ))}
              <td className="income-table__total" data-label="Total">
                {money.brl(row.total)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
