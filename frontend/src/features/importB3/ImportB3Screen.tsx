import { useId, useState, type ChangeEvent } from "react";
import { useB3Preview } from "../../api/b3Import";
import type { B3AssetGroup, B3FileReport, B3Preview } from "../../api/b3ImportTypes";
import { errorMessage } from "../../api/client";
import { GlassCard } from "../../components/GlassCard";
import { MagneticButton } from "../../components/MagneticButton";
import { useToast } from "../../components/toastContext";
import { CLASS_META } from "../../lib/assetClasses";
import { formatBRL, formatQuantity, plural } from "../../lib/format";
import {
  CLASS_BASIS_HINT,
  EVENT_KIND_LABEL,
  FILE_KIND_LABEL,
  FILE_PROBLEM_TEXT,
  INCOME_KIND_LABEL,
  formatBytes,
  formatIsoDate,
  reasonText,
} from "./labels";
import "./importB3.css";

/** Teto do servidor para todos os arquivos de um envio. */
export const MAX_TOTAL_BYTES = 10 * 1024 * 1024;
export const MAX_FILES = 10;

const sameFile = (a: File, b: File) => a.name === b.name && a.size === b.size && a.lastModified === b.lastModified;

export function ImportB3Screen() {
  const inputId = useId();
  const hintId = useId();
  const [files, setFiles] = useState<File[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const preview = useB3Preview();
  const toast = useToast();

  const total = files.reduce((sum, file) => sum + file.size, 0);
  const tooBig = total > MAX_TOTAL_BYTES;

  const pick = (event: ChangeEvent<HTMLInputElement>) => {
    const picked = Array.from(event.target.files ?? []);
    // Deixa escolher o mesmo arquivo de novo depois de tirá-lo da lista.
    event.target.value = "";
    if (picked.length === 0) return;
    const merged = [...files];
    for (const file of picked) if (!merged.some((known) => sameFile(known, file))) merged.push(file);
    setNotice(merged.length > MAX_FILES ? `Máximo de ${MAX_FILES} planilhas por vez.` : null);
    setFiles(merged.slice(0, MAX_FILES));
    preview.reset();
  };

  const remove = (file: File) => {
    setFiles(files.filter((known) => known !== file));
    setNotice(null);
    preview.reset();
  };

  const read = () => {
    preview.mutate(files, { onError: (error) => toast({ tone: "error", message: errorMessage(error) }) });
  };

  return (
    <div className="screen import-b3">
      <header className="screen__header">
        <p className="eyebrow">Importar</p>
        <h1 className="screen__title">Importar da B3</h1>
        <p className="screen__lead">
          Suba as planilhas que a Área do Investidor da B3 exporta e veja o que entraria na carteira. Por enquanto o
          Prumo só lê e mostra: nada é gravado.
        </p>
      </header>

      <GlassCard as="section" className="import-card" aria-labelledby="import-how">
        <h2 id="import-how" className="section-title">
          Como baixar as planilhas
        </h2>
        <ol className="import-steps">
          <li>
            Entre na <strong>Área do Investidor</strong> da B3 (<strong>investidor.b3.com.br</strong>) com o seu login.
          </li>
          <li>
            No menu, abra <strong>Extratos</strong> e escolha <strong>Negociação</strong>. Defina o período e baixe em
            Excel (<strong>.xlsx</strong>).
          </li>
          <li>
            Repita em <strong>Movimentação</strong> e em <strong>Proventos</strong> (os recebidos e os a receber).
          </li>
          <li>
            Se a B3 limitar o período de cada consulta, baixe em partes. O Prumo junta as planilhas e não conta nada em
            dobro.
          </li>
          <li>Envie os arquivos aqui, do jeito que a B3 entregou, sem abrir e salvar de novo.</li>
        </ol>
        <p className="field__hint">
          O histórico da B3 começa por volta de novembro de 2019. A planilha não traz corretagem nem taxas: o preço
          médio sai um pouco abaixo do fiscal. Os arquivos são lidos na hora e não ficam guardados.
        </p>
      </GlassCard>

      <GlassCard as="section" className="import-card" aria-labelledby="import-send">
        <h2 id="import-send" className="section-title">
          Suas planilhas
        </h2>
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
                  <button type="button" className="btn btn--ghost" onClick={() => remove(file)}>
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
            onClick={read}
            disabled={files.length === 0 || tooBig}
            loading={preview.isPending}
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
      </GlassCard>

      {preview.data && <PreviewResult preview={preview.data} />}
    </div>
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

function PreviewResult({ preview }: { preview: B3Preview }) {
  const { summary } = preview;
  const nothing = summary.assets === 0;
  return (
    <>
      <GlassCard as="section" className="import-card" aria-labelledby="import-summary">
        <h2 id="import-summary" className="section-title">
          O que entraria
        </h2>
        <dl className="import-stats">
          <Stat label="Ativos" value={summary.assets} />
          <Stat label="Negócios" value={summary.trades} />
          <Stat label="Proventos" value={summary.income} />
          <Stat label="Eventos" value={summary.events} />
          <Stat label="Não entendidas" value={summary.unrecognized} />
        </dl>
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
          <AssetsTable assets={preview.assets} />
        )}
        <div className="import-actions">
          <MagneticButton disabled aria-describedby="import-next">
            Importar
          </MagneticButton>
          <span id="import-next" className="field__hint">
            Chega na próxima versão. Por enquanto o Prumo só mostra o que entraria; nada foi gravado.
          </span>
        </div>
      </GlassCard>

      {preview.unrecognized.length > 0 && (
        <GlassCard as="section" className="import-card" aria-labelledby="import-unrecognized">
          <h2 id="import-unrecognized" className="section-title">
            Linhas que não entendi ({preview.unrecognized.length})
          </h2>
          <p className="field__hint">Estas linhas não entram. Se importarem para você, lance à mão na Carteira.</p>
          <ul className="import-list">
            {preview.unrecognized.map((row) => (
              <li key={`${row.file}-${row.sheet}-${row.line}`}>
                {preview.files[row.file]?.name ?? "arquivo"} · linha {row.line}:{" "}
                {reasonText(row.reason_code, row.reason)}
                <span className="import-list__text">{row.text}</span>
              </li>
            ))}
          </ul>
        </GlassCard>
      )}

      {preview.ignored.length > 0 && (
        <GlassCard as="section" className="import-card" aria-labelledby="import-ignored">
          <h2 id="import-ignored" className="section-title">
            Linhas deixadas de fora de propósito
          </h2>
          <ul className="import-list">
            {preview.ignored.map((item) => (
              <li key={item.reason_code}>
                {item.count} {plural(item.count, ["linha", "linhas"])}: {reasonText(item.reason_code, item.reason)}
              </li>
            ))}
          </ul>
        </GlassCard>
      )}
    </>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="import-stat">
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function AssetsTable({ assets }: { assets: B3AssetGroup[] }) {
  const [open, setOpen] = useState<string | null>(null);
  return (
    <div className="import-table-wrap">
      <table className="import-table">
        <caption className="field__hint">Agrupado por ativo. A classe é uma sugestão: quem confirma é você.</caption>
        <thead>
          <tr>
            <th scope="col">Ativo</th>
            <th scope="col">Classe sugerida</th>
            <th scope="col">Compras</th>
            <th scope="col">Vendas</th>
            <th scope="col">Saldo dos negócios</th>
            <th scope="col">Proventos recebidos</th>
            <th scope="col">A receber</th>
            <th scope="col">Linhas</th>
          </tr>
        </thead>
        <tbody>
          {assets.map((asset) => (
            <AssetRows
              key={asset.key}
              asset={asset}
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

function AssetRows({ asset, expanded, onToggle }: { asset: B3AssetGroup; expanded: boolean; onToggle: () => void }) {
  const { totals } = asset;
  const detailId = useId();
  const lines = asset.trades.length + asset.events.length + asset.income.length;
  // O Tesouro não tem código: o "key" já é o nome do título.
  const title = asset.treasury ? "Tesouro Direto" : asset.key;
  const subtitle = asset.treasury ? asset.key : asset.name;
  return (
    <>
      <tr>
        <td>
          <span className="import-asset__code">{title}</span>
          {subtitle && <span className="import-asset__name">{subtitle}</span>}
        </td>
        <td>
          {CLASS_META[asset.suggested_class].label}
          <span className={`import-class__hint${asset.class_basis === "code" ? " is-warn" : ""}`}>
            {CLASS_BASIS_HINT[asset.class_basis]}
          </span>
        </td>
        <td>
          {orDash(
            totals.bought_quantity,
            `${formatQuantity(totals.bought_quantity, true)} · ${formatBRL(totals.bought_value)}`,
          )}
        </td>
        <td>
          {orDash(totals.sold_quantity, `${formatQuantity(totals.sold_quantity, true)} · ${formatBRL(totals.sold_value)}`)}
        </td>
        <td>{asset.trades.length > 0 ? formatQuantity(totals.net_quantity, true) : "—"}</td>
        <td>{orDash(totals.income_received, formatBRL(totals.income_received))}</td>
        <td>{orDash(totals.income_provisioned, formatBRL(totals.income_provisioned))}</td>
        <td>
          <button
            type="button"
            className="btn btn--ghost"
            aria-expanded={expanded}
            aria-controls={detailId}
            onClick={onToggle}
          >
            {expanded ? "Esconder" : `Ver ${lines} ${plural(lines, ["linha", "linhas"])}`}
            <span className="sr-only">
              {expanded ? " linhas" : ""} de {title}
            </span>
          </button>
        </td>
      </tr>
      {expanded && (
        <tr id={detailId}>
          <td colSpan={8}>
            <ul className="import-lines" aria-label={`Linhas de ${title}`}>
              {asset.trades.map((trade) => (
                <li key={trade.external_ref}>
                  {formatIsoDate(trade.date)} · {trade.side === "buy" ? "Compra" : "Venda"}
                  {trade.market === "fractional" ? " (fracionário)" : ""} · {formatQuantity(trade.quantity, true)} ×{" "}
                  {formatBRL(trade.unit_price)} = {formatBRL(trade.total)}
                </li>
              ))}
              {asset.events.map((event) => (
                <li key={event.external_ref}>
                  {formatIsoDate(event.date)} · {EVENT_KIND_LABEL[event.kind]} (
                  {event.direction === "credit" ? "entrada" : "saída"})
                  {event.quantity !== null ? ` · ${formatQuantity(event.quantity, true)}` : ""}
                  {event.value !== null ? ` · ${formatBRL(event.value)}` : ""}
                </li>
              ))}
              {asset.income.map((income) => (
                <li key={income.external_ref}>
                  {formatIsoDate(income.date)} · {INCOME_KIND_LABEL[income.kind]} ·{" "}
                  {income.status === "received" ? "recebido" : "a receber"} · {formatBRL(income.net_amount)}
                </li>
              ))}
            </ul>
          </td>
        </tr>
      )}
    </>
  );
}
