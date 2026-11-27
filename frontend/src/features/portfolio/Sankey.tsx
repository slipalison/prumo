import { useMemo } from "react";
import { classColor } from "../../lib/assetClasses";
import type { Composition, CompositionNode } from "../../lib/composition";
import { formatPercent } from "../../lib/format";
import { layoutSankey } from "../../lib/sankey";

const WIDTH = 760;
const HEIGHT = 340;
const HEADING = 22;
const NODE_WIDTH = 10;
const GAP = 5;
const LABEL_ROOM = 190;
/** Distância mínima, na mesma coluna, entre dois rótulos (o texto tem 12 px); o que não cabe fica para a tabela. */
const MIN_LABEL_SPACING = 14;

function fillOf(node: CompositionNode): string {
  return node.assetClass ? classColor(node.assetClass) : "var(--text-subtle)";
}

type SankeyProps = { composition: Composition; label: string };

/**
 * O diagrama de Sankey da composição: carteira → classe → setor → ativo, com a
 * altura de cada nó proporcional ao valor e a faixa na cor da classe. Os
 * rótulos (os que cabem) mostram nome e fatia (percentual, que não revela patrimônio); os
 * valores em reais estão na tabela que acompanha. É um desenho: quem lê o
 * leitor de tela é o `aria-label` e a tabela.
 */
export function Sankey({ composition, label }: SankeyProps) {
  const layout = useMemo(
    () => layoutSankey(composition.nodes, { width: WIDTH, height: HEIGHT, nodeWidth: NODE_WIDTH, gap: GAP, labelRoom: LABEL_ROOM }),
    [composition],
  );
  const boxes = new Map(layout.boxes.map((box) => [box.id, box]));
  const nodes = new Map(composition.nodes.map((node) => [node.id, node]));
  const columnX = new Map<number, number>();
  // Rótulo só onde cabe: um nó pequeno ao lado de outro com rótulo fica sem, e a tabela tem o nome dele.
  const labeled = new Set<string>();
  const lastLabel = new Map<number, number>();
  for (const node of composition.nodes) {
    const box = boxes.get(node.id);
    if (!box) continue;
    columnX.set(node.column, box.x);
    const middle = box.y + box.height / 2;
    const previous = lastLabel.get(node.column);
    if (previous === undefined || middle - previous >= MIN_LABEL_SPACING) {
      labeled.add(node.id);
      lastLabel.set(node.column, middle);
    }
  }

  return (
    <svg className="sankey" viewBox={`0 0 ${WIDTH} ${HEIGHT + HEADING}`} role="img" aria-label={label} preserveAspectRatio="xMidYMid meet">
      {composition.headings.map((heading, column) => (
        <text key={heading} className="sankey__heading" x={columnX.get(column) ?? 0} y={12}>
          {heading}
        </text>
      ))}
      <g transform={`translate(0 ${HEADING})`}>
        {layout.links.map((link) => {
          const node = nodes.get(link.to);
          return node ? <path key={link.id} className="sankey__link" d={link.path} style={{ fill: fillOf(node) }} /> : null;
        })}
        {composition.nodes.map((node) => {
          const box = boxes.get(node.id);
          if (!box) return null;
          const shown = labeled.has(node.id);
          return (
            <g key={node.id}>
              <rect className="sankey__node" x={box.x} y={box.y} width={box.width} height={Math.max(box.height, 1)} rx={2} style={{ fill: fillOf(node) }} />
              {shown && (
                <text className="sankey__label" x={box.x + box.width + 6} y={box.y + box.height / 2} dominantBaseline="central">
                  {node.label}
                  <tspan className="sankey__percent"> {formatPercent(node.percent)}</tspan>
                </text>
              )}
            </g>
          );
        })}
      </g>
    </svg>
  );
}
