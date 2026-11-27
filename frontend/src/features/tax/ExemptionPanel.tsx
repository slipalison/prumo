import type { TaxMonth, TaxMonths } from "../../api/taxTypes";
import { Panel } from "../../components/Layout";
import { LimitBar } from "../../components/LimitBar";
import { formatPercent } from "../../lib/format";
import { useMoney } from "../../lib/privacy";
import { DoubtTip } from "./DoubtTip";
import { monthName } from "./taxText";

/** O mês de hoje, se o ano é o de hoje; senão dezembro. */
function currentMonth(data: TaxMonths): TaxMonth | undefined {
  const [todayYear, todayMonth] = data.today.split("-").map(Number);
  const month = todayYear === data.year ? todayMonth : 12;
  return data.months.find((m) => m.month === month);
}

/**
 * O isentômetro do mês: quanto das vendas de ações já passou do limite em que
 * o lucro é isento. Só a venda de ações e units soma aqui; o resto da regra
 * fica no ⓘ.
 */
export function ExemptionPanel({ data }: Readonly<{ data: TaxMonths }>) {
  const money = useMoney();
  const month = currentMonth(data);
  const stocks = month?.exemptions.find((e) => e.id === "stock_sales");
  const crypto = month?.exemptions.find((e) => e.id === "crypto_sales");
  if (!month || !stocks) return null;
  const name = monthName(month.month);
  const monthTitle = `${name.charAt(0).toUpperCase()}${name.slice(1)} de ${month.year}`;
  const percent = stocks.limit > 0 ? (stocks.sales / stocks.limit) * 100 : 0;
  const cryptoSales = crypto?.sales ?? 0;

  return (
    <Panel
      className="tax-meter"
      title="Isentômetro"
      info="Só a venda de ações e units soma aqui. ETF, BDR e FII não têm essa isenção. Cripto tem a própria, de 35 mil reais em vendas no mês."
      infoLabel="O que entra no isentômetro"
      actions={<span className="tax-meter__month">{monthTitle}</span>}
    >
      <p className="tax-meter__value">
        <strong>{money.brl(stocks.sales)}</strong> em vendas de ações, de {money.brl(stocks.limit)} isentos
      </p>
      <LimitBar
        current={percent}
        limit={100}
        label={`Vendas de ações em ${name}: ${formatPercent(percent)} do limite de isenção`}
      />
      <p className="tax-meter__percent">{formatPercent(percent)} do limite</p>
      <p className={stocks.exempt ? "tax-meter__status" : "tax-meter__status tax-meter__over"}>
        {stocks.exempt ? (
          <>
            Dentro do limite: o lucro das ações em operação comum neste mês é isento. Ainda cabem {money.brl(stocks.remaining)} em
            vendas.
          </>
        ) : (
          "Passou do limite: o lucro das ações neste mês paga 15%."
        )}
        {month.doubts.includes("day_trade_in_stock_exemption") && <DoubtTip doubt="day_trade_in_stock_exemption" />}
      </p>
      {cryptoSales > 0 && crypto && (
        <p className="tax-meter__status">
          Cripto tem a própria: {money.brl(cryptoSales)} vendidos de {money.brl(crypto.limit)} no mês.
        </p>
      )}
    </Panel>
  );
}
