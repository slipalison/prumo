import { useId, useState, type ChangeEvent } from "react";
import { useB3Commit, useB3Preview } from "../../api/b3Import";
import type {
  B3AssetDecision,
  B3AssetGroup,
  B3AssetPlan,
  B3CommitResult,
  B3FileReport,
  B3LineStatus,
  B3Plan,
  B3PreviewResponse,
} from "../../api/b3ImportTypes";
import { ApiError, errorMessage } from "../../api/client";
import type { AssetClass } from "../../api/types";
import { StatusPill, type Status } from "../../components/DataTable";
import { Disclosure } from "../../components/Disclosure";
import { InfoTip } from "../../components/InfoTip";
import { Grid, Page, PageHeader, Panel, StatStrip, type Stat } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { MoneyInput } from "../../components/MoneyInput";
import { QuantityInput } from "../../components/QuantityInput";
import { useToast } from "../../components/toastContext";
import { ASSET_CLASSES, CLASS_META } from "../../lib/assetClasses";
import { plural } from "../../lib/format";
import { useMoney } from "../../lib/privacy";
import {
  ACTION_LABEL,
  CLASS_BASIS_HINT,
  CONFLICT_TEXT,
  EVENT_KIND_LABEL,
  FILE_KIND_LABEL,
  FILE_PROBLEM_TEXT,
  INCOME_KIND_LABEL,
  LINE_STATE_LABEL,
  OPENING_BASIS_TEXT,
  REPLACED_KIND_LABEL,
  formatBytes,
  formatIsoDate,
  lineReason,
  reasonText,
} from "./labels";
import "./importB3.css";

/** Teto do servidor para todos os arquivos de um envio. */
export const MAX_TOTAL_BYTES = 10 * 1024 * 1024;
export const MAX_FILES = 10;

const sameFile = (a: File, b: File) => a.name === b.name && a.size === b.size && a.lastModified === b.lastModified;

type DecisionMap = Record<string, B3AssetDecision>;

export function ImportB3Screen() {
  const inputId = useId();
  const hintId = useId();
  const [files, setFiles] = useState<File[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [decisions, setDecisions] = useState<DecisionMap>({});
  // As decisões com que a prévia na tela foi feita: o `digest` dela vale para essas.
  const [previewedWith, setPreviewedWith] = useState<string | null>(null);
  const [result, setResult] = useState<B3CommitResult | null>(null);
  const preview = useB3Preview();
  const commit = useB3Commit();
  const toast = useToast();

  const total = files.reduce((sum, file) => sum + file.size, 0);
  const tooBig = total > MAX_TOTAL_BYTES;
  const dirty = previewedWith !== null && previewedWith !== JSON.stringify(decisions);

  const startOver = (next: File[]) => {
    setFiles(next);
    setDecisions({});
    setPreviewedWith(null);
    setResult(null);
    preview.reset();
  };

  const pick = (event: ChangeEvent<HTMLInputElement>) => {
    const picked = Array.from(event.target.files ?? []);
    // Deixa escolher o mesmo arquivo de novo depois de tirá-lo da lista.
    event.target.value = "";
    if (picked.length === 0) return;
    const merged = [...files];
    for (const file of picked) if (!merged.some((known) => sameFile(known, file))) merged.push(file);
    setNotice(merged.length > MAX_FILES ? `Máximo de ${MAX_FILES} planilhas por vez.` : null);
    startOver(merged.slice(0, MAX_FILES));
  };

  const remove = (file: File) => {
    setNotice(null);
    startOver(files.filter((known) => known !== file));
  };

  const read = (with_: DecisionMap = decisions) => {
    setResult(null);
    preview.mutate(
      { files, decisions: { assets: with_ } },
      {
        onSuccess: () => setPreviewedWith(JSON.stringify(with_)),
        onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
      },
    );
  };

  const decide = (key: string, patch: B3AssetDecision) => {
    setDecisions((current) => ({ ...current, [key]: { ...current[key], ...patch } }));
  };

  const importNow = () => {
    const plan = preview.data?.plan;
    if (!plan) return;
    commit.mutate(
      { files, decisions: { assets: decisions }, digest: plan.digest },
      {
        onSuccess: (done) => {
          setResult(done);
          setPreviewedWith(null);
          preview.reset();
          toast({ tone: "success", message: "Importação gravada." });
        },
        onError: (error) => {
          toast({ tone: "error", message: errorMessage(error) });
          // A carteira mudou desde a prévia: lê de novo, para a pessoa conferir.
          if (error instanceof ApiError && error.code === "preview_stale") read();
        },
      },
    );
  };

  return (
    <Page className="import-b3">
      <PageHeader
        eyebrow="Importar"
        title="Importar da B3"
        summary="Nada é gravado antes de você apertar Importar, e importar de novo não duplica."
        infoLabel="Sobre a importação"
        info="Suba as planilhas que a Área do Investidor da B3 exporta, confira o que entra na carteira e importe. O Prumo junta as planilhas e não conta nada em dobro."
      />

      <Grid cols="main-aside" alignStart>
        <Panel title="Suas planilhas" className="import-send">
          <div className="import-pick">
            <label htmlFor={inputId} className="sr-only">
              Escolher planilhas da B3
            </label>
            <input
              id={inputId}
              className="import-pick__input"
              type="file"
              multiple
              accept=".xlsx,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
              onChange={pick}
            />
          </div>
          {notice && <p role="status">{notice}</p>}
          {files.length > 0 && (
            <ul className="import-files" aria-label="Arquivos escolhidos">
              {files.map((file) => {
                const report = preview.data?.files.find((r) => r.name === file.name);
                return (
                  <li key={`${file.name}-${file.size}-${file.lastModified}`} className="import-file">
                    <span className="import-file__name">{file.name}</span>
                    <span className="import-file__meta">{formatBytes(file.size)}</span>
                    {report && <FileResult report={report} />}
                    <button type="button" className="btn btn--ghost import-file__remove" onClick={() => remove(file)}>
                      Tirar
                      <span className="sr-only"> {file.name}</span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
          {tooBig && (
            <p role="alert" className="field__hint">
              Os arquivos passam de {formatBytes(MAX_TOTAL_BYTES)} juntos. Tire alguns ou baixe um período menor.
            </p>
          )}
          <div className="import-actions">
            <MagneticButton
              onClick={() => read()}
              disabled={files.length === 0 || tooBig}
              loading={preview.isPending && previewedWith === null}
              aria-describedby={hintId}
            >
              Ler planilhas
            </MagneticButton>
            <span id={hintId} className="field__hint">
              {files.length === 0
                ? "Escolha ao menos uma planilha .xlsx."
                : `${files.length} ${plural(files.length, ["arquivo", "arquivos"])} · ${formatBytes(total)}`}
            </span>
          </div>
        </Panel>

        <Panel title="Como baixar as planilhas">
          <ol className="import-steps">
            <li>
              Entre na <strong>Área do Investidor</strong> da B3 (<strong>investidor.b3.com.br</strong>) com o seu login.
            </li>
            <li>
              Em <strong>Extratos</strong>, abra <strong>Negociação</strong>, defina o período e baixe em Excel (
              <strong>.xlsx</strong>).
            </li>
            <li>
              Repita em <strong>Movimentação</strong> e em <strong>Proventos</strong> (os recebidos e os a receber).
            </li>
            <li>Envie os arquivos aqui, do jeito que a B3 entregou, sem abrir e salvar de novo.</li>
          </ol>
          <Disclosure summary="Limites da B3 e o que a planilha não traz">
            <ul className="import-notes">
              <li>Se a B3 limitar o período de cada consulta, baixe em partes: o Prumo junta as planilhas.</li>
              <li>O histórico da B3 começa por volta de novembro de 2019.</li>
              <li>A planilha não traz corretagem nem taxas: o preço médio sai um pouco abaixo do fiscal.</li>
              <li>Os arquivos são lidos na hora e não ficam guardados: o Importar os envia de novo.</li>
            </ul>
          </Disclosure>
        </Panel>
      </Grid>

      {preview.data && (
        <PreviewResult
          preview={preview.data}
          decisions={decisions}
          decide={decide}
          dirty={dirty}
          refreshing={preview.isPending}
          importing={commit.isPending}
          onRefresh={() => read()}
          onImport={importNow}
        />
      )}
      {result && <CommitResult result={result} />}
    </Page>
  );
}

function FileResult({ report }: { report: B3FileReport }) {
  if (report.status !== "read") {
    const problem = report.problem ? FILE_PROBLEM_TEXT[report.problem] : "Não consegui ler este arquivo.";
    return (
      <span className="import-file__problem" role="alert">
        {problem}
        {report.missing_columns.length > 0 && ` Faltam: ${report.missing_columns.join(", ")}.`}
      </span>
    );
  }
  const kinds = report.kinds.map((kind) => FILE_KIND_LABEL[kind]).join(", ");
  return (
    <span className="import-file__meta">
      {kinds} · {report.rows} {plural(report.rows, ["linha", "linhas"])} · {report.accepted}{" "}
      {plural(report.accepted, ["entra", "entram"])} · {report.unrecognized} não{" "}
      {plural(report.unrecognized, ["entendida", "entendidas"])} · {report.ignored}{" "}
      {plural(report.ignored, ["deixada", "deixadas"])} de fora
    </span>
  );
}

type PreviewProps = {
  preview: B3PreviewResponse;
  decisions: DecisionMap;
  decide: (key: string, patch: B3AssetDecision) => void;
  dirty: boolean;
  refreshing: boolean;
  importing: boolean;
  onRefresh: () => void;
  onImport: () => void;
};

/** O que a gravação grava: lançamentos, proventos novos e os que substituem o provisionado. */
const writesOf = (plan: B3Plan) =>
  plan.summary.transactions_new + plan.summary.income_new + plan.summary.income_replaced + plan.summary.openings;

function importHint(plan: B3Plan, dirty: boolean): string {
  if (dirty) return "Você mudou uma escolha: atualize a prévia para conferir antes de importar.";
  if (!plan.ready) return "Resolva os conflitos e confirme as classes antes de importar.";
  if (writesOf(plan) === 0) return "Nada novo: tudo já está na carteira.";
  const { transactions_new, income_new, income_replaced } = plan.summary;
  const income = income_new + income_replaced;
  return `Grava ${transactions_new} ${plural(transactions_new, ["lançamento", "lançamentos"])} e ${income} ${plural(income, ["provento", "proventos"])}, tudo de uma vez. Importar de novo não duplica.`;
}

function PreviewResult({ preview, decisions, decide, dirty, refreshing, importing, onRefresh, onImport }: PreviewProps) {
  const money = useMoney();
  const { summary, plan } = preview;
  const nothing = summary.assets === 0;
  const planOf = (key: string) => plan.assets.find((asset) => asset.key === key);
  const review = plan.assets.filter(
    (asset) =>
      asset.opening !== null ||
      asset.conflict !== null ||
      asset.class_needs_confirmation ||
      asset.replaced.length > 0 ||
      asset.action === "skip",
  );
  const entering = plan.summary.transactions_new + plan.summary.income_new + plan.summary.income_replaced;
  const existing = plan.summary.transactions_existing + plan.summary.income_existing;
  const found: Stat[] = [
    { label: "Ativos", value: summary.assets },
    { label: "Negócios", value: summary.trades },
    { label: "Proventos", value: summary.income },
    { label: "Eventos", value: summary.events },
    { label: "Não entendidas", value: summary.unrecognized },
  ];
  const against: Stat[] = [
    { label: "Entram", value: entering },
    { label: "Já existem", value: existing },
    { label: "Conflitos", value: plan.summary.conflicts },
    { label: "Ficam de fora", value: plan.summary.not_imported },
  ];
  return (
    <>
      <Panel title="O que entraria" className="import-summary">
        <StatStrip items={found} inset />
        <div role="group" aria-label="Contra a carteira">
          <StatStrip items={against} inset />
        </div>
        {summary.first_date && summary.last_date && (
          <p className="field__hint">
            De {formatIsoDate(summary.first_date)} a {formatIsoDate(summary.last_date)}.
          </p>
        )}
        {nothing ? (
          <p role="status">
            Não achei nenhuma linha que o Prumo consiga importar nestes arquivos. Confira o que cada um disse acima.
          </p>
        ) : (
          <AssetsTable assets={preview.assets} planOf={planOf} lines={plan.lines} decisions={decisions} decide={decide} />
        )}
      </Panel>

      {review.length > 0 && (
        <Panel title="Conferir antes de importar" info="Só aparece o ativo que pede uma decisão sua: classe, saldo inicial, conflito ou o que ficou de fora.">
          <ul className="import-review">
            {review.map((asset) => (
              <AssetReview key={asset.key} asset={asset} decision={decisions[asset.key]} decide={decide} />
            ))}
          </ul>
        </Panel>
      )}

      <Panel title="Importar" className="import-go">
        <div className="import-actions">
          {dirty && (
            <MagneticButton variant="ghost" onClick={onRefresh} loading={refreshing}>
              Atualizar prévia
            </MagneticButton>
          )}
          <MagneticButton
            onClick={onImport}
            disabled={dirty || !plan.ready || writesOf(plan) === 0 || refreshing}
            loading={importing}
            aria-describedby="import-next"
          >
            Importar
          </MagneticButton>
          <span id="import-next" className="field__hint">
            {importHint(plan, dirty)}
          </span>
        </div>
      </Panel>

      {(preview.unrecognized.length > 0 || preview.ignored.length > 0) && (
        <Grid cols={2} alignStart>
          {preview.unrecognized.length > 0 && (
            <Panel
              title={`Linhas que não entendi (${preview.unrecognized.length})`}
              info="Estas linhas não entram. Se importarem para você, lance à mão na Carteira."
              infoLabel="Sobre as linhas que não entendi"
            >
              <ul className="import-list">
                {preview.unrecognized.map((row) => (
                  <li key={`${row.file}-${row.sheet}-${row.line}`}>
                    {preview.files[row.file]?.name ?? "arquivo"} · linha {row.line}: {reasonText(row.reason_code, row.reason)}
                    <span className="import-list__text">{money.hidden ? "Conteúdo da linha oculto." : row.text}</span>
                  </li>
                ))}
              </ul>
            </Panel>
          )}

          {preview.ignored.length > 0 && (
            <Panel title="Linhas deixadas de fora de propósito">
              <ul className="import-list">
                {preview.ignored.map((item) => (
                  <li key={item.reason_code}>
                    {item.count} {plural(item.count, ["linha", "linhas"])}: {reasonText(item.reason_code, item.reason)}
                  </li>
                ))}
              </ul>
            </Panel>
          )}
        </Grid>
      )}
    </>
  );
}

type TableProps = {
  assets: B3AssetGroup[];
  planOf: (key: string) => B3AssetPlan | undefined;
  lines: Record<string, B3LineStatus>;
  decisions: DecisionMap;
  decide: (key: string, patch: B3AssetDecision) => void;
};

function AssetsTable({ assets, planOf, lines, decisions, decide }: TableProps) {
  const [open, setOpen] = useState<string | null>(null);
  return (
    <div className="data-table import-table-wrap">
      <table className="data-table__table import-table">
        <caption className="sr-only">Agrupado por ativo. Em ativo novo, a classe é uma sugestão: quem confirma é você.</caption>
        <thead>
          <tr>
            <th scope="col">Ativo</th>
            <th scope="col">
              <span className="import-table__head">
                Classe
                <InfoTip label="Sobre a classe dos ativos novos" align="start">
                  Em ativo novo, a classe é uma sugestão: quem confirma é você.
                </InfoTip>
              </span>
            </th>
            <th scope="col">Situação</th>
            <th scope="col" className="is-end">
              Compras
            </th>
            <th scope="col" className="is-end">
              Vendas
            </th>
            <th scope="col" className="is-end">
              Saldo dos negócios
            </th>
            <th scope="col" className="is-end">
              Proventos recebidos
            </th>
            <th scope="col" className="is-end">
              A receber
            </th>
            <th scope="col" className="is-end">
              Linhas
            </th>
          </tr>
        </thead>
        <tbody>
          {assets.map((asset) => (
            <AssetRows
              key={asset.key}
              asset={asset}
              plan={planOf(asset.key)}
              lines={lines}
              decision={decisions[asset.key]}
              decide={decide}
              expanded={open === asset.key}
              onToggle={() => setOpen(open === asset.key ? null : asset.key)}
            />
          ))}
        </tbody>
      </table>
    </div>
  );
}

const orDash = (value: number, text: string) => (value > 0 ? text : "—");

/** O título do Tesouro não tem código: o "key" já é o nome dele. */
const titleOf = (asset: { treasury: boolean; key: string }) => (asset.treasury ? "Tesouro Direto" : asset.key);

function LineState({ status }: { status: B3LineStatus | undefined }) {
  if (!status) return null;
  const why = lineReason(status.reason_code);
  return (
    <span className={`import-line__state is-${status.status}`}>
      {" — "}
      {LINE_STATE_LABEL[status.status]}
      {why ? `: ${why}` : ""}
    </span>
  );
}

type RowsProps = {
  asset: B3AssetGroup;
  plan: B3AssetPlan | undefined;
  lines: Record<string, B3LineStatus>;
  decision: B3AssetDecision | undefined;
  decide: (key: string, patch: B3AssetDecision) => void;
  expanded: boolean;
  onToggle: () => void;
};

function AssetRows({ asset, plan, lines, decision, decide, expanded, onToggle }: RowsProps) {
  const money = useMoney();
  const { totals } = asset;
  const detailId = useId();
  const classId = useId();
  const count = asset.trades.length + asset.events.length + asset.income.length;
  const title = titleOf(asset);
  const subtitle = asset.treasury ? asset.key : asset.name;
  const assetClass = decision?.asset_class ?? plan?.asset_class ?? asset.suggested_class;
  const actionStatus: Record<B3AssetPlan["action"], Status> = {
    create: "ok",
    update: "info",
    skip: "off",
    nothing: "off",
  };
  return (
    <>
      <tr>
        <th scope="row">
          <span className="import-asset__code">{title}</span>
          {subtitle && <span className="import-asset__name">{subtitle}</span>}
        </th>
        <td>
          {plan?.class_editable ? (
            <>
              <label htmlFor={classId} className="sr-only">
                Classe de {title}
              </label>
              <select
                id={classId}
                className="text-input import-class__select"
                value={assetClass}
                onChange={(event) => decide(asset.key, { asset_class: event.target.value as AssetClass })}
              >
                {ASSET_CLASSES.map((value) => (
                  <option key={value} value={value}>
                    {CLASS_META[value].label}
                  </option>
                ))}
              </select>
            </>
          ) : (
            CLASS_META[assetClass].label
          )}
          <span className={`import-class__hint${asset.class_basis === "code" ? " is-warn" : ""}`}>
            {plan && !plan.class_editable && plan.action !== "nothing" && plan.asset_id
              ? "a classe do ativo na carteira"
              : CLASS_BASIS_HINT[asset.class_basis]}
          </span>
        </td>
        <td>
          <span className="import-status">
            {plan ? <StatusPill status={actionStatus[plan.action]}>{ACTION_LABEL[plan.action]}</StatusPill> : "—"}
            {plan?.conflict && <StatusPill status="bad">conflito</StatusPill>}
            {plan?.class_needs_confirmation && <StatusPill status="attention">confira a classe</StatusPill>}
          </span>
        </td>
        <td className="is-end">
          {orDash(
            totals.bought_quantity,
            `${money.quantity(totals.bought_quantity, true)} · ${money.brl(totals.bought_value)}`,
          )}
        </td>
        <td className="is-end">
          {orDash(totals.sold_quantity, `${money.quantity(totals.sold_quantity, true)} · ${money.brl(totals.sold_value)}`)}
        </td>
        <td className="is-end">{asset.trades.length > 0 ? money.quantity(totals.net_quantity, true) : "—"}</td>
        <td className="is-end">{orDash(totals.income_received, money.brl(totals.income_received))}</td>
        <td className="is-end">{orDash(totals.income_provisioned, money.brl(totals.income_provisioned))}</td>
        <td className="is-end">
          <button
            type="button"
            className="btn btn--ghost import-table__lines"
            aria-expanded={expanded}
            aria-controls={detailId}
            onClick={onToggle}
          >
            {expanded ? "Esconder" : `Ver ${count} ${plural(count, ["linha", "linhas"])}`}
            <span className="sr-only">
              {expanded ? " linhas" : ""} de {title}
            </span>
          </button>
        </td>
      </tr>
      {expanded && (
        <tr id={detailId}>
          <td colSpan={9}>
            <ul className="import-lines" aria-label={`Linhas de ${title}`}>
              {asset.trades.map((trade) => (
                <li key={trade.external_ref}>
                  {formatIsoDate(trade.date)} · {trade.side === "buy" ? "Compra" : "Venda"}
                  {trade.market === "fractional" ? " (fracionário)" : ""} · {money.quantity(trade.quantity, true)} ×{" "}
                  {money.brl(trade.unit_price)} = {money.brl(trade.total)}
                  <LineState status={lines[trade.external_ref]} />
                </li>
              ))}
              {asset.events.map((event) => (
                <li key={event.external_ref}>
                  {formatIsoDate(event.date)} · {EVENT_KIND_LABEL[event.kind]} (
                  {event.direction === "credit" ? "entrada" : "saída"})
                  {event.quantity !== null ? ` · ${money.quantity(event.quantity, true)}` : ""}
                  {event.value !== null ? ` · ${money.brl(event.value)}` : ""}
                  <LineState status={lines[event.external_ref]} />
                </li>
              ))}
              {asset.income.map((income) => (
                <li key={income.external_ref}>
                  {formatIsoDate(income.date)} · {INCOME_KIND_LABEL[income.kind]} ·{" "}
                  {income.status === "received" ? "recebido" : "a receber"} · {money.brl(income.net_amount)}
                  <LineState status={lines[income.external_ref]} />
                </li>
              ))}
            </ul>
          </td>
        </tr>
      )}
    </>
  );
}

type ReviewProps = {
  asset: B3AssetPlan;
  decision: B3AssetDecision | undefined;
  decide: (key: string, patch: B3AssetDecision) => void;
};

function AssetReview({ asset, decision, decide }: ReviewProps) {
  const money = useMoney();
  const ids = { quantity: useId(), price: useId(), skip: useId() };
  const title = titleOf(asset);
  const fractional = asset.treasury;
  const opening = asset.opening;
  const chosen = decision?.opening;
  const quantity = chosen?.quantity ?? opening?.quantity ?? 0;
  const price = chosen ? (chosen.unit_price ?? 0) : (opening?.unit_price ?? 0);
  const skipped = decision?.skip ?? asset.action === "skip";
  const setOpening = (next: { quantity?: number; unit_price?: number }) => {
    const q = next.quantity ?? quantity;
    const p = next.unit_price ?? price;
    decide(asset.key, { opening: { quantity: q, unit_price: p > 0 ? p : null } });
  };
  return (
    <li className="import-review__item" aria-label={title}>
      <h3 className="import-review__title">
        {title}
        {asset.treasury || asset.display_name === asset.key ? "" : ` · ${asset.display_name}`}
      </h3>
      {!skipped && asset.final_quantity !== null && (
        <p className="field__hint">
          Na carteira hoje: {money.quantity(asset.current_quantity, fractional)} · depois de importar:{" "}
          {money.quantity(asset.final_quantity, fractional)}
        </p>
      )}
      {!skipped && asset.class_needs_confirmation && (
        <div className="import-review__row">
          <p className="gold">
            A classe veio só pelo código: {CLASS_META[asset.asset_class].label}. Confira na tabela ou confirme.
          </p>
          <button
            type="button"
            className="btn btn--ghost"
            onClick={() => decide(asset.key, { asset_class: asset.asset_class })}
          >
            Confirmar {CLASS_META[asset.asset_class].label}
            <span className="sr-only"> para {title}</span>
          </button>
        </div>
      )}
      {!skipped && opening && (
        <div className="import-review__opening">
          <p>
            Saldo inicial em {formatIsoDate(opening.date)}: {money.quantity(opening.quantity, fractional)} a{" "}
            {opening.unit_price === null ? "custo desconhecido" : `${money.brl(opening.unit_price)} cada`}.
          </p>
          <p className="field__hint">{OPENING_BASIS_TEXT[opening.basis]}</p>
          <div className="import-review__fields">
            <QuantityInput
              id={ids.quantity}
              label={`Saldo inicial de ${title}`}
              value={quantity}
              fractional={fractional}
              onChange={(value) => setOpening({ quantity: value })}
            />
            <MoneyInput
              id={ids.price}
              label={`Preço médio de ${title} (zero se não souber)`}
              value={price}
              onChange={(value) => setOpening({ unit_price: value })}
            />
          </div>
        </div>
      )}
      {!skipped && asset.replaced.length > 0 && (
        <div>
          <p className="field__hint">O que você tinha lançado à mão dá lugar ao histórico da B3:</p>
          <ul className="import-list">
            {asset.replaced.map((entry, index) => (
              <li key={`${entry.kind}-${entry.date}-${index}`}>
                {REPLACED_KIND_LABEL[entry.kind] ?? entry.kind} de {money.quantity(entry.quantity, fractional)} em{" "}
                {formatIsoDate(entry.date)}
              </li>
            ))}
          </ul>
        </div>
      )}
      {!skipped && asset.conflict && (
        <p className="no" role="alert">
          {CONFLICT_TEXT[asset.conflict.reason_code] ?? CONFLICT_TEXT.invalid}
        </p>
      )}
      <div className="switch-row">
        <label htmlFor={ids.skip} className="switch-row__label">
          Deixar {title} de fora desta importação
        </label>
        <input
          id={ids.skip}
          type="checkbox"
          role="switch"
          className="switch"
          checked={skipped}
          onChange={(event) => decide(asset.key, { skip: event.target.checked })}
        />
      </div>
    </li>
  );
}

function CommitResult({ result }: { result: B3CommitResult }) {
  const { summary } = result;
  const reasons = new Map<string, number>();
  for (const line of result.not_imported) reasons.set(line.reason_code, (reasons.get(line.reason_code) ?? 0) + 1);
  const income = summary.income_new + summary.income_replaced;
  return (
    <Panel title="Importação concluída">
      <p role="status">
        Entraram {summary.transactions_new} {plural(summary.transactions_new, ["lançamento", "lançamentos"])} e {income}{" "}
        {plural(income, ["provento", "proventos"])}
        {summary.income_replaced > 0
          ? ` (${summary.income_replaced} ${plural(summary.income_replaced, ["substituiu um provento a receber", "substituíram proventos a receber"])})`
          : ""}
        .
      </p>
      <StatStrip
        inset
        items={[
          { label: "Ativos novos", value: summary.assets_created },
          { label: "Ativos atualizados", value: summary.assets_updated },
          { label: "Saldos iniciais", value: summary.openings },
          { label: "Já existiam", value: summary.transactions_existing + summary.income_existing },
        ]}
      />
      <h3 className="import-review__title">O que ficou de fora</h3>
      <ul className="import-list">
        {[...reasons].map(([code, count]) => (
          <li key={code}>
            {count} {plural(count, ["linha", "linhas"])}: {lineReason(code) ?? code}
          </li>
        ))}
        {result.skipped_assets.length > 0 && (
          <li>
            {result.skipped_assets.length} {plural(result.skipped_assets.length, ["ativo", "ativos"])} que você deixou de
            fora: {result.skipped_assets.join(", ")}
          </li>
        )}
        <li>
          {result.unrecognized} {plural(result.unrecognized, ["linha não entendida", "linhas não entendidas"])} e{" "}
          {result.ignored} {plural(result.ignored, ["deixada", "deixadas"])} de fora de propósito na leitura
        </li>
      </ul>
      <p className="field__hint">
        A Carteira, a Renda e o caixa já mostram o que entrou. O Desempenho refaz a série desde o lançamento mais
        antigo, em segundo plano.
      </p>
    </Panel>
  );
}
