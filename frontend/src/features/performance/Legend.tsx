/** A legenda dos gráficos do Desempenho: o traço (cor e tracejado) e o nome; o número, quando há, em texto. */

export type LegendItem = { key: string; label: string; color: string; dash?: string | undefined; value?: string };

export function Swatch({ color, dash }: { color: string; dash?: string | undefined }) {
  return (
    <svg className="legend__swatch" width="26" height="10" viewBox="0 0 26 10" aria-hidden="true">
      <line x1="1" x2="25" y1="5" y2="5" stroke={color} strokeWidth="2.5" strokeDasharray={dash} strokeLinecap="round" />
    </svg>
  );
}

export function Legend({ items }: { items: LegendItem[] }) {
  return (
    <ul className="legend">
      {items.map((item) => (
        <li key={item.key} className="legend__item">
          <Swatch color={item.color} dash={item.dash} />
          <span>{item.label}</span>
          {item.value && <strong>{item.value}</strong>}
        </li>
      ))}
    </ul>
  );
}
