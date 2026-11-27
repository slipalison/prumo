import type { CompositionNode } from "./composition";

/*
 * O desenho do diagrama de Sankey da composição. A árvore (cada nó tem um pai
 * só) dispensa o algoritmo geral: os nós de uma coluna seguem a ordem do pai,
 * então as faixas nunca se cruzam. Só geometria, sem React.
 */

export type SankeyBox = { id: string; x: number; y: number; width: number; height: number };

export type SankeyLink = {
  /** O nó de destino (a faixa leva a cor da classe dele). */
  id: string;
  from: string;
  to: string;
  /** O contorno da faixa (curva de Bézier nas duas bordas). */
  path: string;
};

export type SankeyLayout = {
  width: number;
  height: number;
  /** Distância entre uma coluna e a seguinte. */
  step: number;
  boxes: SankeyBox[];
  links: SankeyLink[];
};

/** Largura estimada de um texto de 12 px em negrito (o SVG não se mede fora do navegador); erra para mais. */
export function estimateTextWidth(text: string): number {
  return text.length * 6.4;
}

/**
 * Corta o nome com reticências para `nome + fatia` caber em `maxWidth`. A fatia
 * (percentual) nunca é cortada. O nome inteiro fica no `<title>` e na tabela.
 */
export function fitLabel(name: string, percent: string, maxWidth: number): { text: string; truncated: boolean } {
  const room = maxWidth - estimateTextWidth(` ${percent}`);
  if (estimateTextWidth(name) <= room) return { text: name, truncated: false };
  const chars = Math.max(1, Math.floor(room / 6.4) - 1);
  return { text: `${name.slice(0, chars).trimEnd()}…`, truncated: true };
}

/**
 * Quais nós levam rótulo: por coluna, do maior para o menor, só se o centro
 * ficar a `spacing` ou mais de todo rótulo já aceito na coluna.
 */
export function labelledIds(nodes: CompositionNode[], boxes: SankeyBox[], spacing: number): Set<string> {
  const byId = new Map(boxes.map((b) => [b.id, b]));
  const accepted = new Set<string>();
  const centers = new Map<number, number[]>();
  const order = [...nodes].sort((a, b) => b.value - a.value);
  for (const node of order) {
    const box = byId.get(node.id);
    if (!box) continue;
    const middle = box.y + box.height / 2;
    const taken = centers.get(node.column) ?? [];
    if (taken.every((c) => Math.abs(c - middle) >= spacing)) {
      accepted.add(node.id);
      taken.push(middle);
      centers.set(node.column, taken);
    }
  }
  return accepted;
}

export type SankeyOptions = {
  width: number;
  height: number;
  nodeWidth: number;
  /** Espaço vertical entre nós da mesma coluna. */
  gap: number;
  /** Espaço reservado à direita da última coluna para os rótulos. */
  labelRoom: number;
};

const round = (n: number) => Math.round(n * 100) / 100;

export function layoutSankey(nodes: CompositionNode[], options: SankeyOptions): SankeyLayout {
  const { width, height, nodeWidth, gap, labelRoom } = options;
  const total = nodes[0]?.value ?? 0;
  const columnCount = Math.max(...nodes.map((n) => n.column), 0) + 1;
  if (nodes.length === 0 || total <= 0) return { width, height, step: 0, boxes: [], links: [] };

  const columns = Array.from({ length: columnCount }, (_, c) => nodes.filter((n) => n.column === c));
  // A escala é a da coluna mais apertada (a com mais espaços entre nós).
  const scale = Math.min(...columns.map((col) => (height - gap * (col.length - 1)) / total));
  const step = columnCount > 1 ? (width - labelRoom - nodeWidth) / (columnCount - 1) : 0;

  const boxes = new Map<string, SankeyBox>();
  columns.forEach((col, c) => {
    const used = col.reduce((sum, n) => sum + n.value * scale, 0) + gap * (col.length - 1);
    let y = (height - used) / 2;
    for (const node of col) {
      const h = node.value * scale;
      boxes.set(node.id, { id: node.id, x: round(c * step), y: round(y), width: nodeWidth, height: round(h) });
      y += h + gap;
    }
  });

  // Cada pai entrega a altura dos filhos de cima para baixo, na ordem da coluna.
  const cursor = new Map<string, number>();
  const links: SankeyLink[] = [];
  for (const node of nodes) {
    if (node.parent === null) continue;
    const from = boxes.get(node.parent);
    const to = boxes.get(node.id);
    if (!from || !to) continue;
    const offset = cursor.get(node.parent) ?? 0;
    cursor.set(node.parent, offset + to.height);
    const x0 = from.x + from.width;
    const x1 = to.x;
    const mid = (x0 + x1) / 2;
    const top0 = from.y + offset;
    const top1 = to.y;
    const bottom0 = top0 + to.height;
    const bottom1 = top1 + to.height;
    links.push({
      id: node.id,
      from: node.parent,
      to: node.id,
      path:
        `M${round(x0)} ${round(top0)}C${round(mid)} ${round(top0)} ${round(mid)} ${round(top1)} ${round(x1)} ${round(top1)}` +
        `L${round(x1)} ${round(bottom1)}C${round(mid)} ${round(bottom1)} ${round(mid)} ${round(bottom0)} ${round(x0)} ${round(bottom0)}Z`,
    });
  }
  return { width, height, step, boxes: [...boxes.values()], links };
}
