import { useEffect, useMemo, useRef, useState } from "react";
import type { Asset } from "../../api/types";
import { DataTable, type DataColumn } from "../../components/DataTable";
import { Disclosure } from "../../components/Disclosure";
import { Panel } from "../../components/Layout";
import { CLASS_META, classColor } from "../../lib/assetClasses";
import { buildComposition, LEVEL_LABEL, leavesByClass, type CompositionLevel, type CompositionNode } from "../../lib/composition";
import { cx } from "../../lib/cx";
import { formatPercent, plural } from "../../lib/format";
import { useMoney } from "../../lib/privacy";
import { Sankey } from "./Sankey";
import { useNarrow } from "./useNarrow";
import "./panel.css";

const LEVELS: CompositionLevel[] = ["sector", "segment", "asset"];
type View = "diagram" | "bars";
const VIEWS: Array<{ id: View; label: string }> = [
  { id: "diagram", label: "Diagrama" },
  { id: "bars", label: "Barras" },
];

/** A cor de cada fatia nas barras: as dos setores da Carteira, e um tom neutro para "Outros" e "Sem …". */
function sliceColor(node: CompositionNode, index: number): string {
  if (node.kind === "others") return "var(--sector-other)";
  if (node.label.startsWith("Sem ")) return "var(--sector-none)";
  return `var(--sector-${(index % 7) + 1})`;
}

/** Uma barra empilhada por classe: a largura é a fatia dentro da classe, e a legenda escreve cada nome e fatia. */
function StackedBars({ nodes, headings }: { nodes: CompositionNode[]; headings: string[] }) {
  const groups = leavesByClass({ total: 0, headings, nodes });
  const lastHeading = headings[headings.length - 1]?.toLowerCase() ?? "";
  return (
    <ul className="stacked-bars">
      {groups.map(({ node, leaves }) => (
        <li key={node.id} className="stacked-bars__class" style={{ ["--class-color" as string]: classColor(node.assetClass ?? "br_stocks") }}>
          <div className="stacked-bars__head">
            <span className="stacked-bars__name">
              <span className="dot" style={{ background: "var(--class-color)" }} aria-hidden="true" />
              {node.label}
            </span>
            <span className="stacked-bars__share">{formatPercent(node.percent)} da carteira</span>
          </div>
          <div className="stacked-bars__bar" aria-hidden="true">
            {leaves.map((leaf, index) => (
              <span
                key={leaf.id}
                className="stacked-bars__slice"
                style={{ width: `${(leaf.value / node.value) * 100}%`, background: sliceColor(leaf, index) }}
              />
            ))}
          </div>
          <ul className="sector-legend stacked-bars__legend" aria-label={`${node.label} por ${lastHeading}`}>
            {leaves.map((leaf, index) => (
              <li key={leaf.id} className="sector-legend__item">
                <span className="sector-legend__dot" style={{ background: sliceColor(leaf, index) }} aria-hidden="true" />
                <span className="sector-legend__name">{leaf.label}</span>
                <span className="sector-legend__value">{formatPercent((leaf.value / node.value) * 100)}</span>
              </li>
            ))}
          </ul>
        </li>
      ))}
    </ul>
  );
}

/** Abaixo desta largura de painel o diagrama fica apertado (texto minúsculo): só as barras. */
const MIN_DIAGRAM_WIDTH = 600;

/** A largura do elemento, acompanhando o redimensionamento (0 sem `ResizeObserver`). */
function useWidth<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const element = ref.current;
    if (!element || typeof ResizeObserver === "undefined") return undefined;
    const observer = new ResizeObserver(([entry]) => setWidth(entry?.contentRect.width ?? 0));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return { ref, width };
}

/**
 * Composição da carteira: o diagrama de Sankey (carteira → classe → setor →
 * ativo) com o seletor do último nível, ou as barras empilhadas, que no
 * celular são a única vista (o diagrama pede largura). A tabela embaixo traz
 * cada valor, pelos formatadores que respeitam o olho fechado.
 */
export function CompositionPanel({ assets }: { assets: Asset[] }) {
  const money = useMoney();
  const { ref, width } = useWidth<HTMLDivElement>();
  const narrow = useNarrow() || (width > 0 && width < MIN_DIAGRAM_WIDTH);
  const [level, setLevel] = useState<CompositionLevel>("asset");
  const [chosen, setChosen] = useState<View>("diagram");
  const view: View = narrow ? "bars" : chosen;

  const full = useMemo(() => buildComposition(assets, level, false), [assets, level]);
  const collapsed = useMemo(() => buildComposition(assets, level, true), [assets, level]);

  const rows = full.nodes.filter((n) => n.column > 0);
  const columns: Array<DataColumn<CompositionNode>> = [
    { key: "label", label: "Item", rowHeader: true, cell: (n) => <span className={cx("composition-item", `composition-item--${n.column}`)}>{n.label}</span> },
    { key: "level", label: "Nível", cell: (n) => full.headings[n.column] ?? "" },
    { key: "class", label: "Classe", wideOnly: true, cell: (n) => (n.assetClass ? CLASS_META[n.assetClass].label : "") },
    { key: "value", label: "Valor", align: "end", cell: (n) => money.brl(n.value) },
    { key: "percent", label: "Da carteira", align: "end", cell: (n) => formatPercent(n.percent) },
  ];

  const classes = full.nodes.filter((n) => n.kind === "class");
  const summary = classes.map((c) => `${c.label} ${formatPercent(c.percent)}`).join(", ");

  return (
    <div ref={ref} className="composition-wrap">
    <Panel
      title="Composição"
      className="composition"
      info="De onde vem o patrimônio: da carteira para as classes, de cada classe para os setores (em FIIs, o tipo do fundo) e, se quiser, até o ativo. A altura de cada bloco é a fatia do valor. Setor e segmento vêm de fonte pública (B3 nas ações, CVM nos FIIs); o que ela não classificou aparece como “Sem setor”. É um retrato de hoje, não uma recomendação de alocação."
      infoLabel="Sobre a composição"
      actions={
        <div className="composition__controls">
          <div className="chips" role="group" aria-label="Último nível">
            {LEVELS.map((id) => (
              <button key={id} type="button" className={cx("chip chip--quick", level === id && "is-active")} aria-pressed={level === id} onClick={() => setLevel(id)}>
                {LEVEL_LABEL[id]}
              </button>
            ))}
          </div>
          {!narrow && (
            <div className="chips" role="group" aria-label="Vista">
              {VIEWS.map((v) => (
                <button key={v.id} type="button" className={cx("chip chip--quick", view === v.id && "is-active")} aria-pressed={view === v.id} onClick={() => setChosen(v.id)}>
                  {v.label}
                </button>
              ))}
            </div>
          )}
        </div>
      }
    >
      {full.nodes.length === 0 ? (
        <p className="field__hint">Sem ativo com valor ainda: a composição aparece quando houver posição.</p>
      ) : (
        <>
          {view === "diagram" ? (
            <Sankey composition={collapsed} label={`Composição da carteira até ${LEVEL_LABEL[level].toLowerCase()}: ${summary}`} />
          ) : (
            <StackedBars nodes={full.nodes} headings={full.headings} />
          )}
          <Disclosure summary="Ver como tabela" count={`${rows.length} ${plural(rows.length, ["linha", "linhas"])}`}>
            <DataTable caption="Composição da carteira: valor e fatia de cada item" columns={columns} rows={rows} rowKey={(n) => n.id} />
          </Disclosure>
        </>
      )}
    </Panel>
    </div>
  );
}
