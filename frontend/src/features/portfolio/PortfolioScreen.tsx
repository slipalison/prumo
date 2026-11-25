import { motion } from "motion/react";
import { useState } from "react";
import { errorMessage } from "../../api/client";
import { useRefreshQuotes } from "../../api/queries";
import type { Asset, AssetClass, ClassSummary, Portfolio } from "../../api/types";
import { ClassBar } from "../../components/ClassBar";
import { CountUp } from "../../components/CountUp";
import { Donut } from "../../components/Donut";
import { GlassCard } from "../../components/GlassCard";
import { IconAlert, IconArrowDown, IconPlus, IconRefresh } from "../../components/icons";
import { LiveBadge } from "../../components/LiveBadge";
import { PriceFlash, TrendMark } from "../../components/PriceFlash";
import { ScoreBadge } from "../../components/ScoreBadge";
import { useToast } from "../../components/toastContext";
import { acceptsB3Listing, CLASS_META, classColor, nounOf, unitOf } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { formatPercent, plural } from "../../lib/format";
import { countBelowIdeal, hasIdeal, readIdeal, type IdealReading } from "../../lib/ideal";
import { useLiveStatus } from "../../lib/liveStatus";
import { useMoney } from "../../lib/privacy";
import { assetName, isAboveCeiling, needsRating, ratingIntro } from "../../lib/rating";
import { HealthSummary } from "../health/HealthSummary";
import { formatSignedPercent } from "../ledger/format";
import "../ledger/ledger.css";
import { AddAssetSheet } from "./AddAssetSheet";
import { ContributionHistory } from "./ContributionHistory";
import { EditAssetSheet } from "./EditAssetSheet";
import { SectorBreakdown } from "./SectorBreakdown";

type PortfolioScreenProps = { portfolio: Portfolio };

export function PortfolioScreen({ portfolio }: PortfolioScreenProps) {
  const [adding, setAdding] = useState<{ open: boolean; initialClass?: AssetClass | undefined }>({ open: false });
  const [editingId, setEditingId] = useState<string | null>(null);
  const [justCreated, setJustCreated] = useState<Asset | null>(null);
  const refresh = useRefreshQuotes();
  const toast = useToast();
  const live = useLiveStatus();
  const belowIdeal = countBelowIdeal(portfolio.assets);
  const delayedB3 = portfolio.assets.some((a) => a.listed_on_b3 && a.price_source === "quote");

  const editing =
    portfolio.assets.find((a) => a.id === editingId) ?? (justCreated?.id === editingId ? justCreated : null);
  const shown = portfolio.classes.filter((c) => c.asset_count > 0 || c.target_percent > 0);
  const others = portfolio.classes.filter((c) => c.asset_count === 0 && c.target_percent === 0);

  const refreshQuotes = () => {
    refresh.mutate(undefined, {
      onSuccess: (result) => {
        const failed = result.failed.length ? ` Sem resposta: ${result.failed.join(", ")}.` : "";
        toast({
          tone: result.failed.length ? "info" : "success",
          message: `${result.refreshed} ${plural(result.refreshed, ["cotação atualizada", "cotações atualizadas"])}.${failed}`,
        });
      },
      onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
    });
  };

  const openAdd = (initialClass?: AssetClass) => setAdding({ open: true, initialClass });

  return (
    <div className="screen portfolio">
      <header className="screen__header">
        <p className="eyebrow">Carteira</p>
        <h1 className="screen__title">Tudo o que você tem, no eixo certo</h1>
        {hasIdeal(portfolio.assets) && (
          <p className={cx("portfolio__ideal", belowIdeal > 0 && "is-below")}>
            {belowIdeal > 0 && <IconArrowDown width={16} height={16} />}
            {belowIdeal > 0
              ? `${belowIdeal} ${plural(belowIdeal, ["ativo abaixo do ideal", "ativos abaixo do ideal"])}`
              : "Nenhum ativo abaixo do ideal"}
          </p>
        )}
      </header>

      <GlassCard as="section" className="portfolio__hero" aria-label="Patrimônio">
        <div className="portfolio__total">
          <span className="eyebrow">Patrimônio</span>
          <div className="portfolio__total-line">
            <CountUp value={portfolio.total_value} className="display-number" />
            <TrendMark value={portfolio.total_value} />
          </div>
          <div className="portfolio__quotes">
            <LiveBadge status={live} updatedAt={portfolio.quotes_updated_at} delayedB3={delayedB3} />
            <button
              type="button"
              className={cx("icon-btn", refresh.isPending && "is-spinning")}
              aria-label="Atualizar cotações"
              onClick={refreshQuotes}
              disabled={refresh.isPending}
            >
              <IconRefresh />
            </button>
          </div>
        </div>
        <Donut
          size={200}
          label={`Carteira por classe: ${portfolio.classes
            .filter((c) => c.current_value > 0)
            .map((c) => `${CLASS_META[c.asset_class].label} ${formatPercent(c.current_percent)}`)
            .join(", ")}`}
          segments={portfolio.classes.map((c) => ({
            key: c.asset_class,
            value: c.current_value,
            color: classColor(c.asset_class),
            label: CLASS_META[c.asset_class].label,
          }))}
        >
          <strong className="donut__number">{portfolio.assets.length}</strong>
          <span className="donut__caption">{plural(portfolio.assets.length, ["ativo", "ativos"])}</span>
        </Donut>
      </GlassCard>

      <HealthSummary />

      <div className="class-list">
        {shown.map((summary, index) => (
          <ClassSection
            key={summary.asset_class}
            index={index}
            summary={summary}
            assets={portfolio.assets.filter((a) => a.asset_class === summary.asset_class)}
            onEdit={(asset) => setEditingId(asset.id)}
            onAdd={() => openAdd(summary.asset_class)}
          />
        ))}
      </div>

      {others.length > 0 && (
        <GlassCard as="section" className="more-classes" aria-label="Outras classes">
          <p className="field__hint">Sem meta e sem ativo:</p>
          <div className="chips">
            {others.map((c) => (
              <button
                key={c.asset_class}
                type="button"
                className="chip"
                style={{ ["--chip-color" as string]: classColor(c.asset_class) }}
                onClick={() => openAdd(c.asset_class)}
              >
                <IconPlus width={16} height={16} />
                {CLASS_META[c.asset_class].label}
              </button>
            ))}
          </div>
        </GlassCard>
      )}

      <ContributionHistory />

      <motion.button
        type="button"
        className="fab"
        aria-label="Adicionar ativo"
        onClick={() => openAdd()}
        whileHover={{ scale: 1.06, rotate: 90 }}
        whileTap={{ scale: 0.92 }}
      >
        <IconPlus width={28} height={28} />
      </motion.button>

      <AddAssetSheet
        open={adding.open}
        initialClass={adding.initialClass}
        onClose={() => setAdding({ open: false })}
        onCreated={(asset) => {
          setAdding({ open: false });
          // Ativo de Ações/FIIs nasce sem nota e não receberia aporte: a folha
          // de edição já abre no checklist, com a dica no topo.
          if (needsRating(asset)) {
            setJustCreated(asset);
            setEditingId(asset.id);
          }
        }}
      />
      <EditAssetSheet
        asset={editing}
        portfolio={portfolio}
        intro={justCreated && justCreated.id === editingId ? ratingIntro(justCreated) : undefined}
        onClose={() => {
          setEditingId(null);
          setJustCreated(null);
        }}
      />
    </div>
  );
}

type ClassSectionProps = {
  index: number;
  summary: ClassSummary;
  assets: Asset[];
  onEdit: (asset: Asset) => void;
  onAdd: () => void;
};

function ClassSection({ index, summary, assets, onEdit, onAdd }: ClassSectionProps) {
  const meta = CLASS_META[summary.asset_class];
  const headingId = `class-${summary.asset_class}`;

  return (
    <motion.div
      initial={{ opacity: 0, y: 14 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: index * 0.05, type: "spring", stiffness: 260, damping: 26 }}
    >
      <GlassCard
        as="section"
        className="class-card"
        aria-labelledby={headingId}
        style={{ ["--class-color" as string]: classColor(summary.asset_class) }}
      >
        <header className="class-card__head">
          <span className="class-card__dot" aria-hidden="true" />
          <h2 id={headingId} className="class-card__title">
            {meta.label}
          </h2>
          <span className="class-card__numbers">
            {formatPercent(summary.current_percent)} <span className="muted">· meta {formatPercent(summary.target_percent)}</span>
          </span>
        </header>
        <ClassBar assetClass={summary.asset_class} current={summary.current_percent} target={summary.target_percent} />
        <SectorBreakdown assetClass={summary.asset_class} assets={assets} />
        {assets.length === 0 ? (
          <div className="class-card__empty">
            <span className="field__hint">
              {summary.target_percent > 0
                ? "Tem meta e nenhum ativo. Cadastre os que você quer ter, mesmo sem ter comprado, e o aporte chega aqui."
                : "Nenhum ativo."}
            </span>
            <button type="button" className="link-btn" onClick={onAdd}>
              <IconPlus width={16} height={16} /> Adicionar
            </button>
          </div>
        ) : (
          <ul className="asset-list">
            {assets.map((asset) => (
              <li key={asset.id}>
                <AssetRow asset={asset} classAssets={assets} onClick={() => onEdit(asset)} />
              </li>
            ))}
          </ul>
        )}
      </GlassCard>
    </motion.div>
  );
}

function AssetRow({ asset, classAssets, onClick }: { asset: Asset; classAssets: Asset[]; onClick: () => void }) {
  const money = useMoney();
  const ideal = readIdeal(asset, classAssets);
  const unit = unitOf(asset.asset_class, asset.listed_on_b3, asset.treasury);
  const missingPrice = asset.price_source === "missing";
  const price = missingPrice || asset.unit_price === null ? "sem preço" : money.brl(asset.unit_price);
  // Quantidade 0 é ativo da lista, ainda não comprado: recebe aporte pela nota.
  const notOwned = asset.quantity === 0;
  const quantityLine =
    unit === "value"
      ? notOwned
        ? "Ainda não tem"
        : "Valor informado"
      : notOwned
        ? `Ainda não tem · ${price}`
        : `${money.quantity(asset.quantity, unit === "fraction")} ${
            unit === "fraction" && !asset.treasury
              ? (asset.symbol ?? "")
              : plural(asset.quantity, nounOf(asset.asset_class, asset.listed_on_b3, asset.treasury))
          } · ${price}${asset.average_price === null ? "" : ` · PM ${money.brl(asset.average_price)}`}`;
  const b3Tag = asset.listed_on_b3 && acceptsB3Listing(asset.asset_class);

  return (
    <button type="button" className={cx("asset-row", notOwned && "asset-row--wish")} onClick={onClick}>
      <span className="asset-row__id">
        <span className="asset-row__symbol">
          {assetName(asset)}
          {b3Tag && (
            <span className="asset-row__tag" title="Negociado na B3">
              B3
            </span>
          )}
        </span>
        <span className="asset-row__name">
        {asset.treasury ? "Tesouro Direto" : asset.symbol ? asset.display_name : quantityLine}
      </span>
      </span>
      <span className="asset-row__qty">{asset.symbol ? quantityLine : ""}</span>
      <span className="asset-row__value">
        <PriceFlash value={asset.current_value}>{money.brl(asset.current_value)}</PriceFlash>
        {(asset.return_percent !== null || !ideal.counts) && (
          <span className="asset-row__meta">
            {asset.return_percent !== null && <ReturnMark percent={asset.return_percent} />}
            {!ideal.counts && <span className="asset-row__share">{formatPercent(asset.class_percent)} da classe</span>}
          </span>
        )}
      </span>
      <span className="asset-row__score">
        {(missingPrice || asset.score === 0) && (
          <IconAlert
            className="asset-row__warn"
            aria-hidden={false}
            aria-label={missingPrice ? "Sem cotação" : "Sem nota"}
            role="img"
          />
        )}
        <ScoreBadge score={asset.score} />
      </span>
      {ideal.counts && <IdealLine reading={ideal} aboveCeiling={asset.score > 0 && isAboveCeiling(asset)} />}
    </button>
  );
}

/**
 * Atual × ideal do ativo dentro da classe: uma barra fina com um traço no
 * ideal, os dois números por extenso e, abaixo do ideal, o selo com seta e
 * texto (cor nunca é a única pista). Com o preço acima do teto da pessoa, o
 * selo "acima do teto" (ícone e texto) diz por que o ativo não recebe agora. A barra é só desenho; quem lê o leitor de
 * tela é o texto ao lado.
 */
function IdealLine({ reading, aboveCeiling }: { reading: IdealReading; aboveCeiling: boolean }) {
  const scale = Math.min(Math.max(reading.idealShare, reading.currentShare, 1) * 1.15, 100);
  const at = (value: number) => `${Math.min((value / scale) * 100, 100)}%`;
  return (
    <span className="asset-row__ideal">
      <span className="ideal-bar" aria-hidden="true">
        <span className="ideal-bar__fill" style={{ width: at(reading.currentShare) }} />
        <span className="ideal-bar__mark" style={{ left: at(reading.idealShare) }} />
      </span>
      <span className="asset-row__ideal-text">
        {formatPercent(reading.currentShare)} da classe <span className="muted">· ideal {formatPercent(reading.idealShare)}</span>
      </span>
      {reading.below && (
        <span className="below-badge">
          <IconArrowDown width={12} height={12} /> abaixo do ideal
        </span>
      )}
      {/* O teto não muda o ideal (é a carteira que a pessoa quer ter); só
          explica por que, abaixo do ideal ou não, o ativo não recebe agora. */}
      {aboveCeiling && (
        <span className="below-badge ceiling-badge">
          <IconAlert width={12} height={12} /> acima do teto
        </span>
      )}
    </span>
  );
}

/**
 * Rentabilidade da posição sobre o preço médio. O sinal escrito diz o sentido
 * sem depender da cor (a seta ▲/▼ é do preço que acabou de andar, não disto);
 * o leitor de tela ouve "rentabilidade +12,0%". Percentual não revela
 * patrimônio: fica visível com o olho fechado, como o % da classe.
 */
function ReturnMark({ percent }: { percent: number }) {
  const direction = percent > 0 ? "up" : percent < 0 ? "down" : "flat";
  return (
    <span className={cx("asset-row__return", `asset-row__return--${direction}`)} title="Rentabilidade sobre o preço médio">
      <span className="sr-only">rentabilidade </span>
      {formatSignedPercent(percent)}
    </span>
  );
}
