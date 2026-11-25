import type { Asset, AssetClass } from "../../api/types";
import { Donut } from "../../components/Donut";
import { CLASS_META } from "../../lib/assetClasses";
import { formatPercent, plural } from "../../lib/format";
import { sectorHeading, sectorSlices } from "../../lib/sectors";

type SectorBreakdownProps = { assetClass: AssetClass; assets: Asset[] };

const SOURCE_NAME: Partial<Record<AssetClass, string>> = {
  br_stocks: "a B3",
  br_reits: "a CVM",
};

/**
 * Rosca por setor dentro de Ações Nacionais ou de FIIs, com a lista ao lado:
 * o nome e a fatia estão sempre escritos, a cor só ajuda a achar.
 */
export function SectorBreakdown({ assetClass, assets }: SectorBreakdownProps) {
  const slices = sectorSlices(assets, assetClass);
  if (slices.length === 0) return null;

  const heading = sectorHeading(assetClass);
  const classified = slices.filter((s) => s.kind !== "none");
  const sectorCount = slices.filter((s) => s.kind === "sector").length;

  if (classified.length === 0) {
    return (
      <p className="sector-breakdown__empty field__hint">
        Setor indisponível por enquanto: {SOURCE_NAME[assetClass]} ainda não respondeu. O Prumo tenta de novo sozinho.
      </p>
    );
  }

  return (
    <section className="sector-breakdown" aria-label={`${heading} em ${CLASS_META[assetClass].label}`}>
      <Donut
        size={116}
        thickness={13}
        label={`${heading}: ${slices.map((s) => `${s.label} ${formatPercent(s.percent)}`).join(", ")}`}
        segments={slices.map((s) => ({ key: s.key, value: s.value, color: s.color, label: s.label }))}
      >
        <strong className="donut__number donut__number--sm">{sectorCount}</strong>
        <span className="donut__caption">{assetClass === "br_reits" ? plural(sectorCount, ["tipo", "tipos"]) : plural(sectorCount, ["setor", "setores"])}</span>
      </Donut>
      <div className="sector-breakdown__list">
        <h3 className="sector-breakdown__title">{heading}</h3>
        <ul className="sector-legend">
          {slices.map((slice) => (
            <li key={slice.key} className="sector-legend__item">
              <span className="sector-legend__dot" style={{ background: slice.color }} aria-hidden="true" />
              <span className="sector-legend__name">{slice.label}</span>
              <span className="sector-legend__value">{formatPercent(slice.percent)}</span>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
