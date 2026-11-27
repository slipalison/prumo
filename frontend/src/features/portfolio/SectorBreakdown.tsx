import type { Asset, AssetClass } from "../../api/types";
import { Donut } from "../../components/Donut";
import { GlassCard } from "../../components/GlassCard";
import { CLASS_META } from "../../lib/assetClasses";
import { formatPercent, plural } from "../../lib/format";
import { sectorHeading, sectorSlices } from "../../lib/sectors";

type SectorBreakdownProps = { assetClass: AssetClass; assets: Asset[] };

const SOURCE_NAME: Partial<Record<AssetClass, string>> = {
  br_stocks: "a B3",
  br_reits: "a CVM",
};

/**
 * Rosca por setor de Ações Nacionais ou de FIIs, com a lista ao lado, no
 * painel lateral da Carteira: o nome e a fatia estão sempre escritos, a cor só
 * ajuda a achar.
 */
export function SectorBreakdown({ assetClass, assets }: Readonly<SectorBreakdownProps>) {
  const slices = sectorSlices(assets, assetClass);
  if (slices.length === 0) return null;

  const heading = sectorHeading(assetClass);
  const classLabel = CLASS_META[assetClass].label;
  const classified = slices.filter((s) => s.kind !== "none");
  const sectorCount = slices.filter((s) => s.kind === "sector").length;
  const sliceList = slices.map((s) => `${s.label} ${formatPercent(s.percent)}`).join(", ");

  if (classified.length === 0) {
    return (
      <GlassCard as="section" className="panel panel--compact sector-breakdown" aria-label={`${heading} em ${classLabel}`}>
        <p className="field__hint">
          Setor indisponível por enquanto: {SOURCE_NAME[assetClass]} ainda não respondeu. O Prumo tenta de novo sozinho.
        </p>
      </GlassCard>
    );
  }

  return (
    <GlassCard as="section" className="panel sector-breakdown" aria-label={`${heading} em ${classLabel}`}>
      <header className="panel__head">
        <h2 className="panel__title">{heading}</h2>
        <span className="sector-breakdown__class">{classLabel}</span>
      </header>
      <div className="sector-breakdown__body">
        <Donut
          size={104}
          thickness={12}
          label={`${heading}: ${sliceList}`}
          segments={slices.map((s) => ({ key: s.key, value: s.value, color: s.color, label: s.label }))}
        >
          <strong className="donut__number donut__number--sm">{sectorCount}</strong>
          <span className="donut__caption">
            {assetClass === "br_reits" ? plural(sectorCount, ["tipo", "tipos"]) : plural(sectorCount, ["setor", "setores"])}
          </span>
        </Donut>
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
    </GlassCard>
  );
}
