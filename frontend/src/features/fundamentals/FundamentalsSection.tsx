import { useId } from "react";
import { useFundamentals } from "../../api/fundamentals";
import type {
  CompanyFundamentals,
  Figure,
  FundFundamentals,
  PeriodFigures,
  YearCount,
} from "../../api/fundamentalsTypes";
import type { Asset } from "../../api/types";
import { IconAlert } from "../../components/icons";
import { InfoTip } from "../../components/InfoTip";
import { Skeleton } from "../../components/Skeleton";
import { formatDay, formatPercent } from "../../lib/format";
import { formatIsoDate } from "../../lib/income";
import type { Money } from "../../lib/money";
import { useMoney } from "../../lib/privacy";
import { Missing } from "./Missing";
import { NOT_APPLICABLE_TEXT, UNSUPPORTED_TEXT, notApplicableReason } from "./fundamentalsText";
import "./fundamentals.css";

const percent = (figure: Figure) =>
  figure.value === null ? <Missing gap={figure.gap} /> : formatPercent(figure.value * 100);

const times = (figure: Figure) =>
  figure.value === null ? (
    <Missing gap={figure.gap} />
  ) : (
    `${figure.value.toLocaleString("pt-BR", { minimumFractionDigits: 1, maximumFractionDigits: 1 })}×`
  );

const reais = (money: Money, figure: Figure) =>
  figure.value === null ? <Missing gap={figure.gap} /> : money.compact(figure.value);

/** Lucro e patrimônio por ação: em reais com centavos. */
const perShare = (money: Money, figure: Figure) =>
  figure.value === null ? <Missing gap={figure.gap} /> : money.brl(figure.value);

const growth = (figure: Figure) => {
  if (figure.value === null) return <Missing gap={figure.gap} />;
  const sign = figure.value > 0 ? "+" : figure.value < 0 ? "−" : "";
  return `${sign}${formatPercent(Math.abs(figure.value) * 100)} a.a.`;
};

const count = (c: YearCount) => (c.known === 0 ? <Missing gap={c.gap} /> : `${c.positive} de ${c.known}`);

function periodLabel(period: PeriodFigures): string {
  const twelveMonths = period.sources.some((s) => s.kind === "quarterly");
  return twelveMonths
    ? `Últimos 12 meses, até ${formatIsoDate(period.period_end)}`
    : `Exercício encerrado em ${formatIsoDate(period.period_end)}`;
}

function sourceLine(company: CompanyFundamentals): string {
  const filing = company.latest_filing;
  const doc = filing ? `${filing.kind === "annual" ? "DFP" : "ITR"} de ${formatIsoDate(filing.reference_date)}` : "";
  const checked = company.synced_at ? ` · conferido em ${formatDay(company.synced_at)}` : "";
  return `Fonte: CVM${doc ? `, ${doc}` : ""}${checked}`;
}

/** De onde vêm os números do fundo: o informe mensal, a vacância e quando foi conferido. */
function fundSourceLine(fund: FundFundamentals): string {
  const vacancyDate = fund.vacancy_reference_date ? `; vacância do trimestre encerrado em ${formatIsoDate(fund.vacancy_reference_date)}` : "";
  const checked = fund.synced_at ? ` · conferido em ${formatDay(fund.synced_at)}` : "";
  return `Fonte: CVM, informe mensal de ${formatIsoDate(fund.reference_date).slice(3)}${vacancyDate}${checked}.`;
}

function CompanyBody({ company, money }: { company: CompanyFundamentals; money: Money }) {
  const period = company.trailing;
  const reason = notApplicableReason(company.template, company.holding);
  const situation = company.issuer_status && company.issuer_status !== "FASE OPERACIONAL" ? company.issuer_status : null;
  return (
    <>
      {company.judicial_recovery ? (
        <p className="inline-notice">
          <IconAlert width={16} height={16} style={{ verticalAlign: "-3px" }} /> Em recuperação judicial, segundo o cadastro da CVM.
        </p>
      ) : (
        situation && (
          <p className="inline-notice">
            <IconAlert width={16} height={16} style={{ verticalAlign: "-3px" }} /> Situação na CVM: {situation.toLowerCase()}.
          </p>
        )
      )}
      {period && (
        <>
          <span className="field__hint">{periodLabel(period)}</span>
          <dl className="fundamentals__grid">
            <div>
              <dt>Receita líquida</dt>
              <dd>{reais(money, period.revenue)}</dd>
            </div>
            <div>
              <dt>Lucro líquido</dt>
              <dd>{reais(money, period.net_income)}</dd>
            </div>
            <div>
              <dt>ROE</dt>
              <dd>{percent(period.roe)}</dd>
            </div>
            <div>
              <dt>Margem líquida</dt>
              <dd>{percent(period.net_margin)}</dd>
            </div>
            <div>
              <dt>Dív. líquida / EBITDA</dt>
              <dd>{times(period.net_debt_to_ebitda)}</dd>
            </div>
            <div>
              <dt>Dív. líquida / PL</dt>
              <dd>{times(period.net_debt_to_equity)}</dd>
            </div>
            <div>
              <dt>LPA</dt>
              <dd>{perShare(money, company.earnings_per_share)}</dd>
            </div>
            <div>
              <dt>VPA</dt>
              <dd>{perShare(money, company.book_value_per_share)}</dd>
            </div>
          </dl>
        </>
      )}
      <dl className="fundamentals__grid fundamentals__grid--history">
        <div>
          <dt>Anos com lucro</dt>
          <dd>{count(company.profitable_years)}</dd>
        </div>
        <div>
          <dt>Anos com caixa operacional positivo</dt>
          <dd>{count(company.positive_operating_cash_years)}</dd>
        </div>
        <div>
          <dt>Receita, CAGR 5 anos</dt>
          <dd>{growth(company.revenue_cagr)}</dd>
        </div>
        <div>
          <dt>Lucro, CAGR 5 anos</dt>
          <dd>{growth(company.earnings_cagr)}</dd>
        </div>
      </dl>
      {reason && <p className="field__hint">{NOT_APPLICABLE_TEXT[reason]}</p>}
    </>
  );
}

function FundBody({ fund, asset, money }: { fund: FundFundamentals; asset: Asset; money: Money }) {
  // O P/VP acompanha o preço ao vivo da carteira; o do servidor é o da última leitura.
  const price = asset.price_source === "missing" ? null : asset.unit_price;
  const nav = fund.nav_per_share.value;
  const priceToBook: Figure =
    price === null || nav === null || nav <= 0
      ? fund.price_to_book
      : { value: price / nav, gap: null };
  return (
    <>
      <dl className="fundamentals__grid">
        <div>
          <dt>Valor patrimonial por cota</dt>
          <dd>{nav === null ? <Missing gap={fund.nav_per_share.gap} /> : money.brl(nav)}</dd>
        </div>
        <div>
          <dt>P/VP</dt>
          <dd>
            {priceToBook.value === null ? (
              <Missing gap={priceToBook.gap} />
            ) : (
              priceToBook.value.toLocaleString("pt-BR", { minimumFractionDigits: 2, maximumFractionDigits: 2 })
            )}
          </dd>
        </div>
        <div>
          <dt>Vacância física</dt>
          <dd>{percent(fund.vacancy)}</dd>
        </div>
        <div>
          <dt>Patrimônio líquido</dt>
          <dd>{reais(money, fund.net_assets)}</dd>
        </div>
      </dl>
    </>
  );
}

type FundamentalsSectionProps = { asset: Asset };

/**
 * A parte "Fundamentos" da folha do ativo: os números da CVM da companhia ou
 * do FII, com a fonte e a data. Só em Ações Nacionais e FIIs; R$ passa pelo
 * olho de ocultar valores.
 */
export function FundamentalsSection({ asset }: FundamentalsSectionProps) {
  const headingId = useId();
  const query = useFundamentals();
  const money = useMoney();
  if (asset.asset_class !== "br_stocks" && asset.asset_class !== "br_reits") return null;
  const entry = query.data?.assets.find((a) => a.asset_id === asset.id);

  let body;
  let source: string | null = null;
  if (query.isPending) {
    body = <Skeleton height={56} />;
  } else if (query.isError) {
    body = <p className="field__hint">Não consegui carregar os fundamentos agora.</p>;
  } else if (!entry) {
    body = <p className="field__hint">{UNSUPPORTED_TEXT.pending}</p>;
  } else if (entry.kind === "company") {
    body = <CompanyBody company={entry.company} money={money} />;
    source = `${sourceLine(entry.company)}. EBITDA aproximado: EBIT + depreciação e amortização.`;
  } else if (entry.kind === "fund") {
    body = <FundBody fund={entry.fund} asset={asset} money={money} />;
    source = fundSourceLine(entry.fund);
  } else {
    body = <p className="field__hint">{UNSUPPORTED_TEXT[entry.reason]}</p>;
  }

  return (
    <section className="fundamentals" aria-labelledby={headingId}>
      <div className="fundamentals__head">
        <h3 id={headingId} className="section-title">
          Fundamentos
        </h3>
        {source && (
          <InfoTip label="De onde vêm os fundamentos" align="start">
            {source}
          </InfoTip>
        )}
      </div>
      {body}
    </section>
  );
}
