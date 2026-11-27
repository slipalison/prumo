import { useId, useState, type FormEvent, type PointerEvent } from "react";
import { errorMessage } from "../../api/client";
import { useIncomeProjection } from "../../api/projection";
import type {
  IncomeProjection,
  PathSource,
  ProjectedAsset,
  ProjectedGroup,
  ProjectedMonth,
  ProjectionDriver,
  ProjectionGroup,
  ProjectionPremises,
  ProjectionQuery,
} from "../../api/projectionTypes";
import { DataTable, type DataColumn } from "../../components/DataTable";
import { Disclosure } from "../../components/Disclosure";
import { Panel, StatStrip } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { formatPercent, parseDecimal } from "../../lib/format";
import { formatIsoDate } from "../../lib/income";
import type { Money } from "../../lib/money";
import { formatMonth, formatPoints } from "../../lib/objectives";
import { useMoney } from "../../lib/privacy";
import "../objectives/projection.css";
import "./incomeProjection.css";

const GROUP_LABEL: Record<ProjectionGroup, string> = {
  paper_reits: "FII de papel",
  brick_reits: "FII de tijolo",
  other_reits: "FII híbrido ou de fundos",
  br_stocks: "Ações",
  foreign: "Exterior",
};

const DRIVER_LABEL: Record<ProjectionDriver, string> = {
  cdi: "acompanha o CDI esperado",
  ipca: "reajuste pelo IPCA esperado",
  flat: "a média, sem crescimento",
};

type Overrides = Omit<ProjectionQuery, "months">;

/**
 * A renda dos próximos 12 ou 24 meses em três cenários, por tipo de ativo: o
 * FII de papel acompanha o CDI esperado (Selic do Focus), o de tijolo o
 * reajuste pelo IPCA do Focus e o resto fica na média do que pagou. As
 * premissas e a data do Focus ficam à vista, e a pessoa troca as que quiser.
 */
export function IncomeProjectionPanel() {
  const [months, setMonths] = useState<12 | 24>(12);
  const [overrides, setOverrides] = useState<Overrides>({});
  const projection = useIncomeProjection({ months, ...overrides });

  return (
    <Panel
      title="Renda projetada"
      className="income-projection"
      infoLabel="Como a renda é projetada"
      info="Parte da estimativa acima (o que cada cota pagou em 12 meses, líquido do IR retido, vezes as cotas de hoje) e leva para a frente mês a mês, no mesmo calendário dos últimos 12 meses. FII de papel: a renda × o CDI esperado ÷ o CDI médio dos 12 meses que passaram. FII de tijolo: a renda × o IPCA esperado acumulado (o reajuste dos aluguéis). Ações e o resto: a média do que pagou (até 5 anos), sem crescimento. O pessimista e o otimista tiram e somam a amplitude por ano. É um cenário com premissas, não uma promessa; os pagamentos já anunciados estão em Próximos pagamentos."
      actions={
        <fieldset className="proj-toggle" aria-label="Horizonte da projeção">
          {([12, 24] as const).map((m) => (
            <button key={m} type="button" aria-pressed={months === m} onClick={() => setMonths(m)}>
              {m} meses
            </button>
          ))}
        </fieldset>
      }
    >
      {projection.isPending && <Skeleton height={160} radius={16} />}
      {projection.isError && (
        <p className="field__hint" role="alert">
          Não consegui carregar a projeção agora. {errorMessage(projection.error)}
        </p>
      )}
      {projection.data && (
        <Loaded data={projection.data} overrides={overrides} onOverrides={setOverrides} />
      )}
    </Panel>
  );
}

type LoadedProps = { data: IncomeProjection; overrides: Overrides; onOverrides: (next: Overrides) => void };

function Loaded({ data, overrides, onOverrides }: Readonly<LoadedProps>) {
  const money = useMoney();
  const horizon = data.premises.horizon_months;
  if (data.assets.length === 0) {
    return (
      <p className="field__hint">
        Nenhum ativo com estimativa de renda ainda: não há o que projetar. Quando houver histórico de proventos dos ativos,
        a projeção aparece aqui.
      </p>
    );
  }
  return (
    <div className="income-projection__body">
      <StatStrip
        inset
        items={[
          {
            id: "base",
            label: `Base, ${horizon} meses`,
            value: money.brl(data.totals.base),
            detail: `${money.brl(data.totals.base / horizon)} por mês, em média`,
            tone: "gold",
          },
          { id: "low", label: "Pessimista", value: money.brl(data.totals.low) },
          { id: "high", label: "Otimista", value: money.brl(data.totals.high) },
        ]}
      />
      <ProjectedBars months={data.months} />
      <Reconciliation data={data} money={money} />
      <GroupTable data={data} money={money} />
      <Disclosure summary="Por ativo" count={data.assets.length}>
        <AssetTable data={data} money={money} />
      </Disclosure>
      <PremisesLine premises={data.premises} months={data.months} />
      {data.missing.length > 0 && (
        <p className="field__hint">Sem estimativa, fora da conta: {data.missing.join(", ")}.</p>
      )}
      <PremisesForm premises={data.premises} overrides={overrides} onOverrides={onOverrides} />
    </div>
  );
}

const H = 150;
const W = 640;

/** As barras do cenário base, mês a mês, com o traço do pessimista ao otimista. */
function ProjectedBars({ months }: Readonly<{ months: ProjectedMonth[] }>) {
  const money = useMoney();
  const [hover, setHover] = useState<number | null>(null);
  const count = months.length;
  const max = Math.max(...months.map((m) => m.high), 0) * 1.08 || 1;
  const slot = W / Math.max(count, 1);
  const barW = Math.min(slot * 0.6, 40);
  const y = (v: number) => H - (Math.max(v, 0) / max) * H;
  const onMove = (event: PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    if (rect.width <= 0 || count === 0) return;
    const index = Math.floor(((event.clientX - rect.left) / rect.width) * count);
    setHover(Math.min(Math.max(index, 0), count - 1));
  };
  const hovered = hover === null ? undefined : months[hover];
  const left = hover === null ? 0 : ((hover + 0.5) / count) * 100;
  const first = months[0];
  const last = months[count - 1];
  const summary =
    first && last
      ? `Renda projetada no cenário base, de ${formatMonth(first.month)} a ${formatMonth(last.month)}: de ${money.brl(first.base)} a ${money.brl(last.base)} por mês.`
      : "Renda projetada: sem meses.";

  return (
    <figure className="proj-bars">
      <div className="proj-bars__grid">
        <div className="proj-bars__axis" aria-hidden="true">
          <span style={{ top: `${(y(max / 1.08) / H) * 100}%` }}>{money.compact(max / 1.08)}</span>
          <span style={{ top: "100%" }}>{money.compact(0)}</span>
        </div>
        <div
          className="proj-bars__plot"
          onPointerMove={onMove}
          onPointerDown={onMove}
          onPointerLeave={() => setHover(null)}
          data-testid="projection-bars"
        >
          <svg // NOSONAR: gráfico em SVG; role="img" com aria-label é o que o nomeia, não há tag nativa equivalente
            viewBox={`0 0 ${W} ${H}`}
            preserveAspectRatio="none"
            role="img"
            aria-label={summary}
          >
            {months.map((m, i) => {
              const x = i * slot + (slot - barW) / 2;
              const mid = i * slot + slot / 2;
              return (
                <g key={m.month} className={hover === i ? "is-hover" : undefined}>
                  <rect className="proj-bars__bar" x={x} y={y(m.base)} width={barW} height={Math.max(H - y(m.base), 0)} rx={2} />
                  <line className="proj-bars__range" x1={mid} x2={mid} y1={y(m.high)} y2={y(m.low)} />
                </g>
              );
            })}
          </svg>
          {hovered && hover !== null && (
            <div className={`proj-chart__tooltip${left > 55 ? " is-left" : ""}`} style={{ left: `${left}%` }} aria-hidden="true">
              <strong>{formatMonth(hovered.month)}</strong>
              <span>
                Base <b>{money.brl(hovered.base)}</b>
              </span>
              <span>
                Faixa{" "}
                <b>
                  {money.brl(hovered.low)} a {money.brl(hovered.high)}
                </b>
              </span>
              {(Object.entries(hovered.by_group) as Array<[ProjectionGroup, number]>).map(([group, value]) => (
                <span key={group}>
                  {GROUP_LABEL[group]} <b>{money.brl(value)}</b>
                </span>
              ))}
            </div>
          )}
        </div>
        {first && last && (
          <div className="proj-bars__dates" aria-hidden="true">
            <span>{formatMonth(first.month)}</span>
            <span>{formatMonth(last.month)}</span>
          </div>
        )}
      </div>
      <figcaption className="proj-chart__legend">
        <span className="proj-chart__key proj-bars__key--bar">Cenário base</span>
        <span className="proj-chart__key proj-bars__key--range">Pessimista a otimista</span>
      </figcaption>
      <div className="sr-only">
        <table>
          <caption>Renda projetada por mês</caption>
          <thead>
            <tr>
              <th scope="col">Mês</th>
              <th scope="col">Pessimista</th>
              <th scope="col">Base</th>
              <th scope="col">Otimista</th>
            </tr>
          </thead>
          <tbody>
            {months.map((m) => (
              <tr key={m.month}>
                <th scope="row">{formatMonth(m.month)}</th>
                <td>{money.brl(m.low)}</td>
                <td>{money.brl(m.base)}</td>
                <td>{money.brl(m.high)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </figure>
  );
}

function scenarioColumns<T extends { totals: { low: number; base: number; high: number } }>(money: Money): DataColumn<T>[] {
  return [
    { key: "low", label: "Pessimista", align: "end", cell: (row) => money.brl(row.totals.low) },
    { key: "base", label: "Base", align: "end", cell: (row) => <strong>{money.brl(row.totals.base)}</strong> },
    { key: "high", label: "Otimista", align: "end", cell: (row) => money.brl(row.totals.high) },
  ];
}

const START_LABEL = "Ponto de partida, por mês";

/** De onde sai a partida de um ativo: os 12 meses da estimativa ou a média do habitual. */
function basisText(asset: Pick<ProjectedAsset, "basis" | "usual_years">): string {
  return asset.basis === "usual" ? `média de ${asset.usual_years ?? 2} anos` : "12 meses";
}

/** De onde veio a taxa que move a partida: informada pela pessoa, do Focus ou medida. */
function sourceWord(source: PathSource): string {
  if (source === "informed") return "informado";
  if (source === "focus") return "do Focus";
  return "medido";
}

/** O que move a partida depois: o CDI ou o IPCA (do Focus, ou o que a pessoa informou). */
function driverSource(driver: ProjectionDriver, premises: ProjectionPremises): string | null {
  if (driver === "cdi") return `anda com o CDI ${sourceWord(premises.cdi.source)}`;
  if (driver === "ipca") return `anda com o IPCA ${sourceWord(premises.ipca.source)}`;
  return null;
}

/** De onde sai a partida de um tipo: junta a base de cada ativo dele ("média de até 5 anos (3), 12 meses (1)"). */
function groupBasisText(group: ProjectedGroup, assets: ProjectedAsset[]): string {
  const mine = assets.filter((a) => a.group === group.group);
  const usual = mine.filter((a) => a.basis === "usual");
  const trailing = mine.length - usual.length;
  const years = Math.max(...usual.map((a) => a.usual_years ?? 2), 0);
  const average = `média de ${usual.length > 1 ? "até " : ""}${years} anos`;
  if (usual.length === 0) return "12 meses";
  if (trailing === 0) return average;
  return `${average} (${usual.length}), 12 meses (${trailing})`;
}

function StartCell({ value, hint, money }: Readonly<{ value: number; hint: string; money: Money }>) {
  return (
    <span className="income-projection__start">
      {money.brl(value)}
      <span className="income-projection__origin">{hint}</span>
    </span>
  );
}

/** As colunas da tabela por tipo: a partida diz de onde sai e o que a move. */
function groupColumns(data: IncomeProjection, money: Money): DataColumn<ProjectedGroup>[] {
  return [
    { key: "group", label: "Tipo", rowHeader: true, cell: (g) => GROUP_LABEL[g.group] },
    { key: "driver", label: "Como anda", cell: (g) => DRIVER_LABEL[g.driver] },
    {
      key: "start",
      label: START_LABEL,
      align: "end",
      cell: (g) => (
        <StartCell
          value={g.monthly_start}
          money={money}
          hint={[groupBasisText(g, data.assets), driverSource(g.driver, data.premises)].filter(Boolean).join("; ")}
        />
      ),
    },
    ...scenarioColumns<ProjectedGroup>(money),
  ];
}

function GroupTable({ data, money }: Readonly<{ data: IncomeProjection; money: Money }>) {
  return <DataTable caption="Renda projetada por tipo" columns={groupColumns(data, money)} rows={data.groups} rowKey={(g) => g.group} />;
}

/** As colunas da tabela por ativo: a partida diz a base (com os 12 meses, se ela é a média) e o que a move. */
function assetColumns(data: IncomeProjection, money: Money): DataColumn<ProjectedAsset>[] {
  return [
    { key: "asset", label: "Ativo", rowHeader: true, cell: (a) => a.symbol ?? a.display_name },
    { key: "group", label: "Tipo", cell: (a) => GROUP_LABEL[a.group] },
    {
      key: "start",
      label: START_LABEL,
      align: "end",
      cell: (a) => (
        <StartCell
          value={a.monthly_start}
          money={money}
          hint={[
            a.basis === "usual" ? `${basisText(a)} (12 meses: ${money.brl(a.estimate_monthly)})` : basisText(a),
            driverSource(a.driver, data.premises),
          ]
            .filter(Boolean)
            .join("; ")}
        />
      ),
    },
    ...scenarioColumns<ProjectedAsset>(money),
  ];
}

function AssetTable({ data, money }: Readonly<{ data: IncomeProjection; money: Money }>) {
  return <DataTable caption="Renda projetada por ativo" columns={assetColumns(data, money)} rows={data.assets} rowKey={(a) => a.asset_id} />;
}

/** Onde a média do habitual entra: nas ações, no exterior ou nos dois. */
function usualWhere(assets: ProjectedAsset[]): string {
  const groups = new Set(assets.filter((a) => a.basis === "usual").map((a) => a.group));
  if (groups.has("br_stocks") && groups.has("foreign")) return "nas ações e no exterior";
  return groups.has("foreign") ? "no exterior" : "nas ações";
}

/**
 * A conta que liga a estimativa da Renda (logo acima, 12 meses) à partida da
 * projeção: as duas diferem quando ações e exterior partem da média do
 * habitual, ou quando um ativo sem tipo de renda fica de fora. Tudo calculado
 * pelo servidor e escrito pelo `useMoney`.
 */
function Reconciliation({ data, money }: Readonly<{ data: IncomeProjection; money: Money }>) {
  const r = data.reconciliation;
  const estimate = money.brl(data.estimate_monthly);
  if (Math.abs(r.start_monthly - data.estimate_monthly) < 0.005) {
    return (
      <p className="income-projection__reconcile">
        A projeção parte da estimativa de hoje: {estimate} por mês (12 meses).
      </p>
    );
  }
  const parts: string[] = [];
  if (Math.abs(r.usual_difference) >= 0.005) {
    const spans = new Set(data.assets.filter((a) => a.basis === "usual").map((a) => a.usual_years));
    const years = `média de ${spans.size > 1 ? "até " : ""}${r.usual_years ?? 2} anos`;
    parts.push(
      r.usual_difference < 0
        ? `${usualWhere(data.assets)} usa a ${years}, sem o que veio acima do habitual (${money.brl(-r.usual_difference)} por mês a menos)`
        : `${usualWhere(data.assets)} usa a ${years}, acima dos 12 meses, que vieram abaixo do habitual (${money.brl(r.usual_difference)} por mês a mais)`,
    );
  }
  if (Math.abs(r.unprojected_monthly) >= 0.005) {
    parts.push(`deixa de fora ${money.brl(r.unprojected_monthly)} por mês de ativos sem tipo de renda na projeção`);
  }
  return (
    <p className="income-projection__reconcile">
      A estimativa de hoje é {estimate} por mês (12 meses). A projeção parte de {money.brl(r.start_monthly)}
      {parts.length > 0 ? `: ${parts.join("; e ")}.` : "."}
    </p>
  );
}

const pct = (value: number | null) => (value === null ? "—" : formatPercent(value, 2));

/** As premissas que a conta usou, em frases curtas, com a data do Focus. */
function PremisesLine({ premises, months }: Readonly<{ premises: ProjectionPremises; months: ProjectedMonth[] }>) {
  const { cdi, ipca, focus } = premises;
  const end = formatMonth(months.at(-1)?.month ?? null);
  const cdiText = {
    focus: `CDI de ${pct(cdi.now_percent)} hoje a ${pct(cdi.end_percent)} em ${end} (meta Selic do Focus menos 0,10 ponto)`,
    informed: `CDI informado por você: ${pct(cdi.end_percent)} ao ano`,
    measured: `CDI de hoje, parado: ${pct(cdi.now_percent)} (sem Focus no banco)`,
    fallback: `CDI de hoje, parado: ${pct(cdi.now_percent)}`,
    missing: "Sem CDI no banco: a renda de papel fica parada",
  }[cdi.source];
  const ipcaText = {
    focus: `IPCA esperado de ${pct(ipca.annual_percent)} ao ano (Focus)`,
    informed: `IPCA informado por você: ${pct(ipca.annual_percent)} ao ano`,
    measured: `IPCA dos últimos 12 meses: ${pct(ipca.annual_percent)} ao ano (sem Focus no banco)`,
    fallback: `IPCA na meta de inflação: ${pct(ipca.annual_percent)} ao ano (sem Focus nem IPCA no banco)`,
    missing: `IPCA de ${pct(ipca.annual_percent)} ao ano`,
  }[ipca.source];
  return (
    <ul className="income-projection__premises" aria-label="Premissas da projeção">
      <li>{cdiText}.</li>
      {cdi.trailing_percent !== null && <li>Base do FII de papel: o CDI médio dos 12 meses que passaram, {pct(cdi.trailing_percent)}.</li>}
      <li>{ipcaText}.</li>
      <li>
        Cenários: {formatPoints(premises.scenario_spread_percent)} ao ano para cada lado
        {premises.spread_source === "informed" ? " (seu)" : " (padrão)"}.
      </li>
      <li>{focus ? `Focus do Banco Central, pesquisa de ${formatIsoDate(focus.survey_date)}.` : "Ainda sem Focus no banco: a busca é diária."}</li>
    </ul>
  );
}

type Fields = { cdi: string; ipca: string; spread: string };

const LIMITS = { cdi: [0, 50], ipca: [-10, 50], spread: [0, 20] } as const;

function parseField(key: keyof Fields, text: string): { value: number | undefined; error: string | null } {
  if (text.trim() === "") return { value: undefined, error: null };
  const value = parseDecimal(text);
  const [min, max] = LIMITS[key];
  if (value === null || value < min || value > max) return { value: undefined, error: `Use um número de ${min} a ${max}.` };
  return { value, error: null };
}

const toField = (value: number | undefined) => (value === undefined ? "" : String(value).replace(".", ","));

/** Trocar o CDI, o IPCA ou a amplitude da conta, sem gravar nada. */
function PremisesForm({
  premises,
  overrides,
  onOverrides,
}: Readonly<{ premises: ProjectionPremises; overrides: Overrides; onOverrides: (next: Overrides) => void }>) {
  const ids = { cdi: useId(), ipca: useId(), spread: useId() };
  const [fields, setFields] = useState<Fields>({
    cdi: toField(overrides.cdi),
    ipca: toField(overrides.ipca),
    spread: toField(overrides.spread),
  });
  const checks = {
    cdi: parseField("cdi", fields.cdi),
    ipca: parseField("ipca", fields.ipca),
    spread: parseField("spread", fields.spread),
  };
  const valid = !checks.cdi.error && !checks.ipca.error && !checks.spread.error;
  const changed = Object.keys(overrides).length > 0;

  const apply = (event: FormEvent) => {
    event.preventDefault();
    if (!valid) return;
    const next: Overrides = {};
    if (checks.cdi.value !== undefined) next.cdi = checks.cdi.value;
    if (checks.ipca.value !== undefined) next.ipca = checks.ipca.value;
    if (checks.spread.value !== undefined) next.spread = checks.spread.value;
    onOverrides(next);
  };

  const reset = () => {
    setFields({ cdi: "", ipca: "", spread: "" });
    onOverrides({});
  };

  const field = (key: keyof Fields, label: string, placeholder: string) => (
    <div className="field">
      <label htmlFor={ids[key]} className="field__label">
        {label}
      </label>
      <input
        id={ids[key]}
        className="text-input"
        inputMode="decimal"
        placeholder={placeholder}
        value={fields[key]}
        aria-invalid={checks[key].error ? true : undefined}
        onChange={(event) => setFields((current) => ({ ...current, [key]: event.target.value }))}
      />
      {checks[key].error && (
        <span className="rule-row__error" role="alert">
          {checks[key].error}
        </span>
      )}
    </div>
  );

  return (
    <Disclosure summary="Trocar premissas" count={changed ? "trocadas" : undefined}>
      <form className="income-projection__form" onSubmit={apply} noValidate>
        <p className="field__hint">Em branco, vale o Focus (ou o padrão). Uma taxa só para o período inteiro, em % ao ano. Nada é gravado.</p>
        <div className="objective-form__grid">
          {field("cdi", "CDI (% ao ano)", premises.cdi.end_percent === null ? "sem CDI" : `agora: ${pct(premises.cdi.end_percent)}`)}
          {field("ipca", "IPCA (% ao ano)", `agora: ${pct(premises.ipca.annual_percent)}`)}
          {field("spread", "Amplitude (pontos ao ano)", "padrão: 2")}
        </div>
        <div className="income-projection__actions">
          <MagneticButton type="submit" magnetic={false} disabled={!valid}>
            Refazer a conta
          </MagneticButton>
          {changed && (
            <MagneticButton type="button" variant="ghost" magnetic={false} onClick={reset}>
              Voltar ao Focus
            </MagneticButton>
          )}
        </div>
      </form>
    </Disclosure>
  );
}
