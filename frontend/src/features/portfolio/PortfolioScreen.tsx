import { motion } from "motion/react";
import { useState } from "react";
import { errorMessage } from "../../api/client";
import { useRefreshQuotes } from "../../api/queries";
import type { Asset, AssetClass, ClassSummary, Portfolio } from "../../api/types";
import { ClassBar } from "../../components/ClassBar";
import { CountUp } from "../../components/CountUp";
import { Donut } from "../../components/Donut";
import { GlassCard } from "../../components/GlassCard";
import { IconAlert, IconPlus, IconRefresh } from "../../components/icons";
import { ScoreBadge } from "../../components/ScoreBadge";
import { useToast } from "../../components/toastContext";
import { acceptsB3Listing, CLASS_META, classColor, nounOf, unitOf } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { formatBRL, formatPercent, formatQuantity, formatRelative, plural } from "../../lib/format";
import { needsRating, ratingIntro } from "../../lib/rating";
import { AddAssetSheet } from "./AddAssetSheet";
import { ContributionHistory } from "./ContributionHistory";
import { EditAssetSheet } from "./EditAssetSheet";

type PortfolioScreenProps = { portfolio: Portfolio };

export function PortfolioScreen({ portfolio }: PortfolioScreenProps) {
  const [adding, setAdding] = useState<{ open: boolean; initialClass?: AssetClass | undefined }>({ open: false });
  const [editingId, setEditingId] = useState<string | null>(null);
  const [justCreated, setJustCreated] = useState<Asset | null>(null);
  const refresh = useRefreshQuotes();
  const toast = useToast();

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
      </header>

      <GlassCard as="section" className="portfolio__hero" aria-label="Patrimônio">
        <div className="portfolio__total">
          <span className="eyebrow">Patrimônio</span>
          <CountUp value={portfolio.total_value} className="display-number" />
          <div className="portfolio__quotes">
            <span className="field__hint">Cotações {formatRelative(portfolio.quotes_updated_at)}</span>
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
                <AssetRow asset={asset} onClick={() => onEdit(asset)} />
              </li>
            ))}
          </ul>
        )}
      </GlassCard>
    </motion.div>
  );
}

function AssetRow({ asset, onClick }: { asset: Asset; onClick: () => void }) {
  const unit = unitOf(asset.asset_class, asset.listed_on_b3);
  const missingPrice = asset.price_source === "missing";
  const price = missingPrice || asset.unit_price === null ? "sem preço" : formatBRL(asset.unit_price);
  // Quantidade 0 é ativo da lista, ainda não comprado: recebe aporte pela nota.
  const notOwned = asset.quantity === 0;
  const quantityLine =
    unit === "value"
      ? notOwned
        ? "Ainda não tem"
        : "Valor informado"
      : notOwned
        ? `Ainda não tem · ${price}`
        : `${formatQuantity(asset.quantity, unit === "fraction")} ${
            unit === "fraction" ? (asset.symbol ?? "") : plural(asset.quantity, nounOf(asset.asset_class, asset.listed_on_b3))
          } · ${price}`;
  const b3Tag = asset.listed_on_b3 && acceptsB3Listing(asset.asset_class);

  return (
    <button type="button" className={cx("asset-row", notOwned && "asset-row--wish")} onClick={onClick}>
      <span className="asset-row__id">
        <span className="asset-row__symbol">
          {asset.symbol ?? asset.display_name}
          {b3Tag && (
            <span className="asset-row__tag" title="Negociado na B3">
              B3
            </span>
          )}
        </span>
        <span className="asset-row__name">{asset.symbol ? asset.display_name : quantityLine}</span>
      </span>
      <span className="asset-row__qty">{asset.symbol ? quantityLine : ""}</span>
      <span className="asset-row__value">
        {formatBRL(asset.current_value)}
        <span className="asset-row__share">{formatPercent(asset.class_percent)} da classe</span>
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
    </button>
  );
}
