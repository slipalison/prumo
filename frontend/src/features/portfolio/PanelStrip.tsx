import { usePanelMonth } from "../../api/panel";
import type { PanelMonth } from "../../api/panelTypes";
import type { Portfolio } from "../../api/types";
import { Panel, StatStrip, type Stat } from "../../components/Layout";
import type { Money } from "../../lib/money";
import { compare, directionWord, monthName, patrimonyChange, previousMonthStart, signDirection, type Direction } from "../../lib/panel";
import { useMoney } from "../../lib/privacy";
import { formatSignedPercent, signedBRL } from "../ledger/format";
import { formatDay } from "../performance/performanceFormat";
import { DeltaArrow } from "./DeltaArrow";
import "./panel.css";

const toneOf = (direction: Direction): Stat["tone"] => (direction === "above" ? "up" : direction === "below" ? "down" : undefined);

/** O valor de um indicador: a seta e o número, juntos. */
function Figure({ direction, children }: { direction: Direction; children: string }) {
  return (
    <span className="panel-figure">
      <DeltaArrow direction={direction} />
      {children}
    </span>
  );
}

/** Dinheiro do mês contra o do mês anterior: a palavra e o valor, ou nada com os valores ocultos. */
function versusMoney(money: Money, current: number, previous: number, previousName: string): string {
  if (money.hidden) return `${previousName} inteiro, valores ocultos`;
  const word = directionWord(compare(current, previous));
  return word === "igual" ? `igual a ${previousName} (${money.brl(previous)})` : `${word} de ${previousName} (${money.brl(previous)})`;
}

function patrimonyStat(month: PanelMonth, portfolio: Portfolio, money: Money, previousName: string): Stat {
  const change = patrimonyChange(portfolio.total_value, month.previous_month_end?.value ?? null);
  const label = "Patrimônio no mês";
  const info =
    "Quanto o patrimônio andou desde o fechamento do último dia do mês anterior. Inclui os aportes e os proventos recebidos: o que é aporte aparece no indicador ao lado, e a rentabilidade (que descarta o aporte) no seguinte.";
  if (!change) return { id: "patrimony", label, value: "—", detail: "A carteira ainda não tem um fim de mês para comparar.", info };
  const direction = signDirection(change.delta);
  return {
    id: "patrimony",
    label,
    value: <Figure direction={direction}>{signedBRL(money, change.delta)}</Figure>,
    detail: `${formatSignedPercent(change.percent)} · ${directionWord(direction)} do fim de ${previousName}`,
    tone: money.hidden ? undefined : toneOf(direction),
    info,
  };
}

function returnStat(month: PanelMonth, name: string, previousName: string): Stat {
  const info =
    "Rentabilidade do mês pelo método de cotas (TWR): só o que preço e proventos fizeram, sem o aporte. Mede do fechamento do mês anterior até o último dia da série.";
  const current = month.current_return;
  const label = `Rentabilidade em ${name}`;
  if (!current) return { id: "return", label, value: "—", detail: "Ainda sem dias para medir neste mês.", info };
  const previous = month.previous_return;
  const since = current.full_window ? "" : ` · desde ${formatDay(current.from)}`;
  const direction = signDirection(current.return_percent, 0.005);
  let detail = `Sem ${previousName} para comparar${since}`;
  if (previous) {
    const versus = compare(current.return_percent, previous.return_percent, 0.005);
    const word = versus === "equal" ? "igual a" : `${directionWord(versus)} de`;
    detail = `${word} ${previousName} (${formatSignedPercent(previous.return_percent)})${since}`;
  }
  return {
    id: "return",
    label,
    value: <Figure direction={direction}>{formatSignedPercent(current.return_percent)}</Figure>,
    detail,
    tone: toneOf(direction),
    info,
  };
}

function moneyStat(
  id: "income" | "contribution",
  label: string,
  info: string,
  month: PanelMonth,
  money: Money,
  previousName: string,
): Stat {
  const now = month.current[id];
  const before = month.previous[id];
  const nothing = now === 0 && before === 0;
  return {
    id,
    label,
    value: money.brl(now),
    detail: nothing ? `Nada neste mês nem em ${previousName}` : versusMoney(money, now, before, previousName),
    tone: id === "income" ? "gold" : undefined,
    info,
  };
}

/**
 * A faixa de indicadores do topo da Carteira: patrimônio, rentabilidade,
 * proventos e aporte do mês, cada um contra o mês anterior. O sentido vem
 * sempre escrito ("acima", "abaixo", sinal) além da seta e da cor. Com os
 * valores ocultos, os de reais viram máscara e a comparação deles some (diria
 * se o mês foi maior ou menor); os percentuais ficam, como no resto da tela.
 * Enquanto carrega, sem série ou se falhar, não aparece.
 */
export function PanelStrip({ portfolio }: { portfolio: Portfolio }) {
  const month = usePanelMonth();
  const money = useMoney();
  const data = month.data;
  if (!data || data.as_of === null) return null;

  const name = monthName(data.month_starts_on);
  const previousName = monthName(previousMonthStart(data.month_starts_on));
  const items: Stat[] = [
    patrimonyStat(data, portfolio, money, previousName),
    returnStat(data, name, previousName),
    moneyStat(
      "income",
      `Proventos em ${name}`,
      "Proventos líquidos (já sem imposto retido) com pagamento no mês, até hoje. O mês anterior está inteiro: a comparação de um mês em andamento com um mês fechado tende a mostrar menos.",
      data,
      money,
      previousName,
    ),
    moneyStat(
      "contribution",
      `Aporte em ${name}`,
      "O que entrou na carteira no mês pelos lançamentos de compra e venda: aportes menos retiradas, no valor pago. O mês anterior está inteiro.",
      data,
      money,
      previousName,
    ),
  ];
  return (
    <Panel compact aria-label="Indicadores do mês" className="panel-strip">
      <StatStrip items={items} inset />
    </Panel>
  );
}
