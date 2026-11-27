/** De que lado da marca o nome fica. */
export type Side = "right" | "left" | "top" | "bottom";

/** Uma marca do gráfico, em pixels do desenho. */
export type Mark = {
  key: string;
  label: string;
  kind: "asset" | "portfolio" | "benchmark";
  /** Peso na carteira (só os ativos): o maior escolhe o lugar primeiro. */
  weight?: number;
  cx: number;
  cy: number;
  r: number;
};

type Box = { x0: number; y0: number; x1: number; y1: number };

/** O rótulo: 12 px, seminegrito (≈ 7 px por letra), 16 px de altura. */
const LABEL_H = 16;
const CHAR_W = 7;

const overlaps = (a: Box, b: Box) => a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1;

/** A ordem de escolha do lugar do nome: a carteira, o Ibovespa e então os ativos. */
const KIND_RANK: Record<Mark["kind"], number> = { portfolio: 0, benchmark: 1, asset: 2 };

/**
 * Onde vai o nome de cada marca, sem encostar em outra marca nem em outro
 * nome: a carteira e o Ibovespa primeiro, depois os ativos do maior peso para
 * o menor; cada um tenta à direita, à esquerda, em cima e embaixo, sem sair do
 * desenho. A carteira e o Ibovespa sempre levam o nome: sem lugar livre, ele
 * pode passar por cima de uma bolha (nunca de outro nome). Ativo que não cabe
 * fica sem nome à vista (aparece no ponteiro, e está na tabela por ativo).
 */
export function placeLabels(marks: Mark[], width: number, height: number): Map<string, Side> {
  const placed = new Map<string, Side>();
  const taken: Box[] = marks.map(({ cx, cy, r }) => ({ x0: cx - r, y0: cy - r, x1: cx + r, y1: cy + r }));
  const order = [...marks].sort((a, b) => KIND_RANK[a.kind] - KIND_RANK[b.kind] || (b.weight ?? 0) - (a.weight ?? 0));
  const labels: Box[] = [];
  for (const { key, label, kind, cx, cy, r } of order) {
    const w = label.length * CHAR_W + 2;
    const options: Array<[Side, Box]> = [
      ["right", { x0: cx + r + 4, y0: cy - LABEL_H / 2, x1: cx + r + 4 + w, y1: cy + LABEL_H / 2 }],
      ["left", { x0: cx - r - 4 - w, y0: cy - LABEL_H / 2, x1: cx - r - 4, y1: cy + LABEL_H / 2 }],
      ["top", { x0: cx - w / 2, y0: cy - r - 2 - LABEL_H, x1: cx + w / 2, y1: cy - r - 2 }],
      ["bottom", { x0: cx - w / 2, y0: cy + r + 2, x1: cx + w / 2, y1: cy + r + 2 + LABEL_H }],
    ];
    const inside = ([, box]: [Side, Box]) =>
      box.x0 >= 0 && box.x1 <= width && box.y0 >= -LABEL_H / 2 && box.y1 <= height + LABEL_H / 2;
    // Nenhuma opção encosta na própria marca: todas começam depois do raio.
    const fit =
      options.find((option) => inside(option) && !taken.some((other) => overlaps(option[1], other))) ??
      (kind === "asset"
        ? undefined
        : options.find((option) => inside(option) && !labels.some((other) => overlaps(option[1], other))));
    if (fit) {
      placed.set(key, fit[0]);
      taken.push(fit[1]);
      labels.push(fit[1]);
    }
  }
  return placed;
}
