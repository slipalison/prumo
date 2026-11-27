import { motion } from "motion/react";
import { useState } from "react";
import { errorMessage } from "../../api/client";
import { useRefreshQuotes } from "../../api/queries";
import type { Asset, AssetClass, ClassSummary, Portfolio } from "../../api/types";
import { AssetIcon } from "../../components/AssetIcon";
import { ClassBar } from "../../components/ClassBar";
import { CountUp } from "../../components/CountUp";
import { Disclosure } from "../../components/Disclosure";
import { Donut } from "../../components/Donut";
import { GlassCard } from "../../components/GlassCard";
import { IconAlert, IconArrowDown, IconPlus, IconRefresh } from "../../components/icons";
import { InfoTip } from "../../components/InfoTip";
import { Grid, Page, PageHeader, Panel } from "../../components/Layout";
import { LiveBadge } from "../../components/LiveBadge";
import { MagneticButton } from "../../components/MagneticButton";
import { PriceFlash, TrendMark } from "../../components/PriceFlash";
import { ScoreBadge } from "../../components/ScoreBadge";
import { useToast } from "../../components/toastContext";
import { acceptsB3Listing, CLASS_META, classColor, nounOf, unitOf } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { formatPercent, plural } from "../../lib/format";
import { countBelowIdeal, hasIdeal, readIdeal, type IdealReading } from "../../lib/ideal";
import { useLiveStatus } from "../../lib/liveStatus";
import { SPRING, STAGGER } from "../../lib/motion";
import { useMoney } from "../../lib/privacy";
import { assetName, isAboveCeiling, needsRating, ratingIntro } from "../../lib/rating";
import { AnalysisSummary } from "../analysis/AnalysisSummary";
import { formatSignedPercent } from "../ledger/format";
import "../ledger/ledger.css";
import { AddAssetSheet } from "./AddAssetSheet";
import { ContributionHistory } from "./ContributionHistory";
import { useNarrow } from "./useNarrow";
import { EditAssetSheet } from "./EditAssetSheet";
import { PanelDetails } from "./PanelDetails";
import { PanelStrip } from "./PanelStrip";
import { ProfitSummary } from "./ProfitSummary";
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
  const sectorClasses = shown.filter((c) => c.asset_count > 0);
  const narrow = useNarrow();

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

  const sectors = sectorClasses.map((c) => (
    <SectorBreakdown
      key={c.asset_class}
      assetClass={c.asset_class}
      assets={portfolio.assets.filter((a) => a.asset_class === c.asset_class)}
    />
  ));

  const openAdd = (initialClass?: AssetClass) => setAdding({ open: true, initialClass });

  return (
    <Page className="portfolio">
      <PageHeader
        title="Tudo o que você tem, no eixo certo"
        infoLabel="Sobre a Carteira"
        info="Cada classe tem a meta que você definiu e a fatia que ocupa hoje. Toque num ativo para editar, lançar compra ou venda, ver os fundamentos e dar a nota."
        actions={
          <MagneticButton onClick={() => openAdd()}>
            <IconPlus width={18} height={18} /> Adicionar ativo
          </MagneticButton>
        }
      />

      <PanelStrip portfolio={portfolio} />

      <div className="portfolio__strip">
        <GlassCard as="section" className="stat-card stat-card--hero" aria-label="Patrimônio">
          <span className="stat-card__label">Patrimônio</span>
          <div className="portfolio__total-line">
            <CountUp value={portfolio.total_value} className="display-number" />
            <TrendMark value={portfolio.total_value} />
          </div>
          <div className="portfolio__quotes">
            <LiveBadge status={live} updatedAt={portfolio.quotes_updated_at} delayedB3={delayedB3} />
            <button
              type="button"
              className={cx("icon-btn icon-btn--sm", refresh.isPending && "is-spinning")}
              aria-label="Atualizar cotações"
              onClick={refreshQuotes}
              disabled={refresh.isPending}
            >
              <IconRefresh />
            </button>
          </div>
        </GlassCard>

        <ProfitSummary profit={portfolio.profit} />

        {hasIdeal(portfolio.assets) && <BelowIdealStat count={belowIdeal} />}

        <AnalysisSummary />
      </div>

      <Grid cols="main-aside" alignStart>
        <div className="portfolio__classes">
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

          {others.length > 0 && (
            <GlassCard as="section" className="more-classes" aria-label="Outras classes">
              <p className="more-classes__label">Sem meta e sem ativo:</p>
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
        </div>

        <div className="portfolio__aside">
          <ClassDonut portfolio={portfolio} />
          {narrow ? (
            // No celular o setor é secundário: fica recolhido, a um toque.
            <Disclosure summary="Por setor e por tipo de fundo" className="portfolio__sectors">
              {sectors}
            </Disclosure>
          ) : (
            sectors
          )}
          <ContributionHistory />
        </div>
      </Grid>

      <PanelDetails portfolio={portfolio} />

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
    </Page>
  );
}

/** Quantos ativos estão abaixo da fatia ideal da própria classe. */
function BelowIdealStat({ count }: { count: number }) {
  return (
    <GlassCard as="section" className={cx("stat-card", count > 0 && "is-below")} aria-label="Abaixo do ideal">
      <span className="stat-card__label">
        Abaixo do ideal
        <InfoTip label="Sobre o ideal de cada ativo" align="start">
          O ideal de um ativo é a fatia que o aporte mira dentro da classe: a nota dele dividida pela soma das notas de quem
          recebe aporte. Está abaixo quando passa de 0,5 ponto percentual aquém.
        </InfoTip>
      </span>
      <strong className="stat-card__value">
        {count > 0 && <IconArrowDown width={16} height={16} />}
        {count > 0 ? `${count} ${plural(count, ["ativo", "ativos"])}` : "Nenhum ativo"}
      </strong>
      <span className="stat-card__detail">{count > 0 ? "abaixo do ideal da classe" : "abaixo do ideal"}</span>
    </GlassCard>
  );
}

/** A rosca por classe, com a legenda escrita: o nome e a fatia nunca dependem da cor. */
function ClassDonut({ portfolio }: { portfolio: Portfolio }) {
  const held = portfolio.classes.filter((c) => c.current_value > 0);
  return (
    <Panel title="Por classe" className="class-donut">
      <div className="class-donut__body">
        <Donut
          size={112}
          thickness={13}
          label={`Carteira por classe: ${held
            .map((c) => `${CLASS_META[c.asset_class].label} ${formatPercent(c.current_percent)}`)
            .join(", ")}`}
          segments={portfolio.classes.map((c) => ({
            key: c.asset_class,
            value: c.current_value,
            color: classColor(c.asset_class),
            label: CLASS_META[c.asset_class].label,
          }))}
        >
          <strong className="donut__number donut__number--sm">{portfolio.assets.length}</strong>
          <span className="donut__caption">{plural(portfolio.assets.length, ["ativo", "ativos"])}</span>
        </Donut>
        <ul className="sector-legend class-donut__legend">
          {held.map((c) => (
            <li key={c.asset_class} className="sector-legend__item">
              <span className="sector-legend__dot" style={{ background: classColor(c.asset_class) }} aria-hidden="true" />
              <span className="sector-legend__name">{CLASS_META[c.asset_class].label}</span>
              <span className="sector-legend__value">{formatPercent(c.current_percent)}</span>
            </li>
          ))}
        </ul>
      </div>
    </Panel>
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
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ ...SPRING.enter, delay: index * STAGGER }}
    >
      <GlassCard
        as="section"
        className="panel panel--flush class-panel"
        aria-labelledby={headingId}
        style={{ ["--class-color" as string]: classColor(summary.asset_class) }}
      >
        <header className="class-panel__head">
          <h2 id={headingId} className="class-panel__title">
            <span className="class-card__dot" aria-hidden="true" />
            {meta.label}
          </h2>
          <div className="class-panel__bar">
            <ClassBar assetClass={summary.asset_class} current={summary.current_percent} target={summary.target_percent} />
          </div>
          <span className="class-panel__numbers">
            {formatPercent(summary.current_percent)} <span className="muted">· meta {formatPercent(summary.target_percent)}</span>
          </span>
        </header>
        {assets.length === 0 ? (
          <div className="class-card__empty">
            <span className="field__hint">
              {summary.target_percent > 0 ? "Tem meta e nenhum ativo." : "Nenhum ativo."}
              {summary.target_percent > 0 && (
                <InfoTip label="Sobre classe com meta e sem ativo" align="start">
                  Cadastre os ativos que você quer ter, mesmo sem ter comprado, e o aporte chega aqui.
                </InfoTip>
              )}
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

/**
 * Uma linha por ativo, no desenho de uma tabela: ativo, quantidade · preço ·
 * PM, valor, rentabilidade, fatia da classe × ideal e nota. A linha inteira é
 * um botão que abre a folha; no celular as mesmas peças empilham em duas ou
 * três linhas (container query do painel, ver `portfolio.css`).
 */
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
    <button type="button" className={cx("asset-row", asset.symbol && "asset-row--ticker", notOwned && "asset-row--wish")} onClick={onClick}>
      <span className="asset-row__id">
        <AssetIcon
          symbol={asset.symbol}
          name={asset.display_name}
          assetClass={asset.asset_class}
          treasury={asset.treasury}
          size="sm"
        />
        <span className="asset-row__text">
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
      </span>
      <span className="asset-row__qty">{asset.symbol ? quantityLine : ""}</span>
      <span className="asset-row__value">
        <PriceFlash value={asset.current_value}>{money.brl(asset.current_value)}</PriceFlash>
      </span>
      <span className="asset-row__return-cell">{asset.return_percent !== null && <ReturnMark percent={asset.return_percent} />}</span>
      {ideal.counts ? (
        <IdealLine reading={ideal} aboveCeiling={asset.score > 0 && isAboveCeiling(asset)} />
      ) : (
        <span className="asset-row__ideal">
          <span className="asset-row__share">{formatPercent(asset.class_percent)} da classe</span>
        </span>
      )}
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

/**
 * Atual × ideal do ativo dentro da classe: uma barra fina com um traço no
 * ideal, os dois números por extenso e, abaixo do ideal, o selo com seta e
 * texto (cor nunca é a única pista). Com o preço acima do teto da pessoa, o
 * selo "acima do teto" (ícone e texto) diz por que o ativo não recebe agora. A
 * barra é só desenho; quem lê o leitor de tela é o texto ao lado.
 */
function IdealLine({ reading, aboveCeiling }: { reading: IdealReading; aboveCeiling: boolean }) {
  const scale = Math.min(Math.max(reading.idealShare, reading.currentShare, 1) * 1.15, 100);
  const at = (value: number) => `${Math.min((value / scale) * 100, 100)}%`;
  return (
    <span className="asset-row__ideal">
      <span className="asset-row__ideal-text">
        {formatPercent(reading.currentShare)} da classe <span className="muted">· ideal {formatPercent(reading.idealShare)}</span>
      </span>
      <span className="ideal-bar" aria-hidden="true">
        <span className="ideal-bar__fill" style={{ width: at(reading.currentShare) }} />
        <span className="ideal-bar__mark" style={{ left: at(reading.idealShare) }} />
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
