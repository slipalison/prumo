import { motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import type { CompanyResults } from "../../api/resultsTypes";
import { formatPercent, plural } from "../../lib/format";
import { SPRING } from "../../lib/motion";
import { placeLabels, type Side } from "../risk/labels";
import { axisDomain, bubblePoint, MODE_LABEL, signedPercent, type BubbleMode } from "./resultsText";

export type CompanyEntry = { assetId: string; symbol: string; weight: number | null; company: CompanyResults };

const FALLBACK_WIDTH = 640;
const NARROW = 480;
const PAD = { left: 52, right: 16, top: 18, bottom: 42 };

/** A largura do gráfico em pixels: desenha na escala real, e o texto não estica. */
function useWidth() {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(FALLBACK_WIDTH);
  useEffect(() => {
    const node = ref.current;
    if (!node || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(([entry]) => {
      const next = Math.round(entry?.contentRect.width ?? 0);
      if (next > 0) setWidth(next);
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  return { ref, width };
}

/** Passos "redondos" do eixo (5%, 10%, 25%...), uns quatro no eixo inteiro. */
function ticks([lo, hi]: [number, number]): number[] {
  const raw = (hi - lo) / 4;
  const step = [0.05, 0.1, 0.2, 0.25, 0.5, 1].find((s) => s >= raw) ?? 1;
  const out: number[] = [];
  for (let t = Math.ceil(lo / step) * step; t <= hi + 1e-9; t += step) out.push(Math.round(t * 1000) / 1000);
  return out;
}

/** Onde escrever o nome no SVG, pelo lado que o arranjo escolheu. */
function labelSpot(side: Side, cx: number, cy: number, r: number): { x: number; y: number; anchor: "start" | "end" | "middle" } {
  switch (side) {
    case "right":
      return { x: cx + r + 4, y: cy + 4, anchor: "start" };
    case "left":
      return { x: cx - r - 4, y: cy + 4, anchor: "end" };
    case "top":
      return { x: cx, y: cy - r - 5, anchor: "middle" };
    case "bottom":
      return { x: cx, y: cy + r + 13, anchor: "middle" };
  }
}

const axisText = (fraction: number) => (fraction === 0 ? "0%" : signedPercent(fraction).replace(",0%", "%"));

type BubblesProps = { entries: CompanyEntry[]; mode: BubbleMode };

/**
 * Lucro × linha de cima do trimestre mais novo: cada empresa é uma bolha, com
 * x = variação do lucro, y = variação da receita (em banco e seguradora, do
 * resultado bruto) e o tamanho = o peso na carteira. Só entra quem tem as duas
 * variações em percentual; prejuízo numa ponta não vira percentual e fica na
 * lista ao lado, com o rótulo. Uma tabela escondida traz os mesmos números.
 */
export function ResultsBubbles({ entries, mode }: BubblesProps) {
  const { ref, width } = useWidth();
  const height = width < NARROW ? 260 : 320;
  const placed: Array<CompanyEntry & { x: number; y: number }> = [];
  const out: Array<{ symbol: string; reason: string }> = [];
  for (const entry of entries) {
    const point = bubblePoint(entry.company, mode);
    if ("out" in point) out.push({ symbol: entry.symbol, reason: point.out });
    else placed.push({ ...entry, ...point });
  }
  const xDomain = axisDomain(placed.map((p) => p.x));
  const yDomain = axisDomain(placed.map((p) => p.y));
  const plotW = Math.max(width - PAD.left - PAD.right, 10);
  const plotH = height - PAD.top - PAD.bottom;
  const sx = (v: number) => PAD.left + ((Math.min(Math.max(v, xDomain[0]), xDomain[1]) - xDomain[0]) / (xDomain[1] - xDomain[0])) * plotW;
  const sy = (v: number) => PAD.top + (1 - (Math.min(Math.max(v, yDomain[0]), yDomain[1]) - yDomain[0]) / (yDomain[1] - yDomain[0])) * plotH;
  const heaviest = Math.max(0, ...placed.map((p) => p.weight ?? 0));
  const maxR = width < NARROW ? 18 : 26;
  const radius = (weight: number | null) => 6 + (maxR - 6) * (heaviest > 0 ? Math.sqrt((weight ?? 0) / heaviest) : 0);
  // As maiores atrás, para a pequena não sumir; e escolhem antes onde vai o nome.
  const ordered = [...placed].sort((a, b) => (b.weight ?? 0) - (a.weight ?? 0));
  const clippedOf = (p: (typeof placed)[number]) =>
    p.x < xDomain[0] || p.x > xDomain[1] || p.y < yDomain[0] || p.y > yDomain[1];
  const labelOf = (p: (typeof placed)[number]) => (clippedOf(p) ? `${p.symbol} ${signedPercent(p.x)}` : p.symbol);
  // O mesmo arranjo de nomes do risco × retorno: o maior peso escolhe primeiro;
  // quem não cabe fica sem nome à vista (está no `title` e na tabela escondida).
  const sides = placeLabels(
    ordered.map((p) => ({
      key: p.assetId,
      label: labelOf(p),
      kind: "asset" as const,
      weight: p.weight ?? 0,
      cx: sx(p.x),
      cy: sy(p.y),
      r: radius(p.weight),
    })),
    width,
    PAD.top + plotH,
  );
  const summary =
    placed.length === 0
      ? `Nenhuma empresa com as duas variações em percentual, ${MODE_LABEL[mode]}.`
      : `${placed.length} ${plural(placed.length, ["empresa", "empresas"])}: variação do lucro no eixo horizontal, da receita no vertical, ${MODE_LABEL[mode]}; o tamanho é o peso na carteira.`;

  return (
    <figure className="results-bubbles">
      <div ref={ref} className="results-bubbles__plot">
        <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label={summary} className="results-bubbles__svg">
          {ticks(xDomain).map((t) => (
            <g key={`x${t}`}>
              <line className={t === 0 ? "results-bubbles__zero" : "results-bubbles__grid"} x1={sx(t)} x2={sx(t)} y1={PAD.top} y2={PAD.top + plotH} />
              <text className="results-bubbles__tick" x={sx(t)} y={PAD.top + plotH + 16} textAnchor="middle">
                {axisText(t)}
              </text>
            </g>
          ))}
          {ticks(yDomain).map((t) => (
            <g key={`y${t}`}>
              <line className={t === 0 ? "results-bubbles__zero" : "results-bubbles__grid"} x1={PAD.left} x2={PAD.left + plotW} y1={sy(t)} y2={sy(t)} />
              <text className="results-bubbles__tick" x={PAD.left - 8} y={sy(t) + 4} textAnchor="end">
                {axisText(t)}
              </text>
            </g>
          ))}
          <text className="results-bubbles__axis" x={PAD.left + plotW} y={height - 4} textAnchor="end">
            lucro →
          </text>
          <text className="results-bubbles__axis" x={PAD.left + 4} y={PAD.top - 6}>
            ↑ receita
          </text>
          {ordered.map((p) => {
            const r = radius(p.weight);
            const cx = sx(p.x);
            const cy = sy(p.y);
            const side = sides.get(p.assetId);
            const spot = side ? labelSpot(side, cx, cy, r) : null;
            return (
              <g key={p.assetId}>
                <motion.circle
                  className="results-bubbles__dot"
                  initial={false}
                  animate={{ cx, cy, r }}
                  transition={SPRING.value}
                >
                  <title>{`${p.symbol}: lucro ${signedPercent(p.x)}, receita ${signedPercent(p.y)}, peso ${formatPercent((p.weight ?? 0) * 100)}`}</title>
                </motion.circle>
                {spot && (
                  <motion.text
                    className="results-bubbles__label"
                    initial={false}
                    animate={{ x: spot.x, y: spot.y }}
                    transition={SPRING.value}
                    textAnchor={spot.anchor}
                  >
                    {labelOf(p)}
                  </motion.text>
                )}
              </g>
            );
          })}
        </svg>
      </div>
      <figcaption className="results-bubbles__caption">
        <span>
          Lucro (horizontal) × receita (vertical) · bolha = peso na carteira · acima e à direita do zero, receita e lucro
          subiram
        </span>
        {out.length > 0 && (
          <span className="results-bubbles__out">
            Fora do gráfico: {out.map((o) => `${o.symbol} (${o.reason})`).join("; ")}.
          </span>
        )}
      </figcaption>
      <div className="sr-only">
        <table>
          <caption>Variação do lucro e da receita, {MODE_LABEL[mode]}</caption>
          <thead>
            <tr>
              <th scope="col">Empresa</th>
              <th scope="col">Lucro</th>
              <th scope="col">Receita</th>
              <th scope="col">Peso na carteira</th>
            </tr>
          </thead>
          <tbody>
            {placed.map((p) => (
              <tr key={p.assetId}>
                <th scope="row">{p.symbol}</th>
                <td>{signedPercent(p.x)}</td>
                <td>{signedPercent(p.y)}</td>
                <td>{formatPercent((p.weight ?? 0) * 100)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </figure>
  );
}
