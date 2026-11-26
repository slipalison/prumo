import { useId, type HTMLAttributes, type ReactNode } from "react";
import { cx } from "../lib/cx";
import { GlassCard } from "./GlassCard";
import { InfoTip } from "./InfoTip";

/*
 * As primitivas de layout das telas (docs/layout.md). Uma tela redesenhada é
 * `<Page>` + `<PageHeader>` + `<Grid>` de `<Panel>`s; números lado a lado vão
 * numa `<StatStrip>` e listas longas numa `<DataTable>`.
 */

type PageProps = HTMLAttributes<HTMLDivElement> & { children: ReactNode };

/**
 * A raiz de uma tela no layout novo: ocupa a largura larga do conteúdo (até
 * `--content-max`) e é o contêiner que as grades consultam.
 */
export function Page({ className, children, ...rest }: PageProps) {
  return (
    <div className={cx("page", className)} {...rest}>
      {children}
    </div>
  );
}

type PageHeaderProps = {
  title: ReactNode;
  /** O rótulo pequeno acima do título (a tela ou a seção). */
  eyebrow?: string;
  /** Uma linha curta abaixo do título: um resumo que ajuda a decidir, não uma explicação. */
  summary?: ReactNode;
  /** A explicação da tela, que antes era um parágrafo: abre no ⓘ ao lado do título. */
  info?: ReactNode;
  /** O nome do ⓘ; o padrão é "Sobre esta tela". */
  infoLabel?: string;
  /** Botões e filtros da tela, à direita no desktop. */
  actions?: ReactNode;
  /** `id` do título, para `aria-labelledby` de quem precisar. */
  titleId?: string;
};

/** O topo da tela: título em uma linha, o resto sob demanda ou à direita. */
export function PageHeader({ title, eyebrow, summary, info, infoLabel = "Sobre esta tela", actions, titleId }: PageHeaderProps) {
  return (
    <header className="page-header">
      <div className="page-header__titles">
        {eyebrow && <span className="eyebrow">{eyebrow}</span>}
        <div className="page-header__heading">
          <h1 id={titleId} className="page-header__title">
            {title}
          </h1>
          {info && <InfoTip label={infoLabel} align="start" placement="bottom">{info}</InfoTip>}
        </div>
        {summary && <p className="page-header__summary">{summary}</p>}
      </div>
      {actions && <div className="page-header__actions">{actions}</div>}
    </header>
  );
}

export type GridLayout = "auto" | 2 | 3 | 4 | "main-aside" | "aside-main";

type GridProps = HTMLAttributes<HTMLElement> & {
  as?: "div" | "section" | "ul" | "ol";
  /**
   * - `2`, `3`, `4`: colunas iguais; uma coluna até 640 px de conteúdo, duas
   *   até 1000 px, depois o número pedido.
   * - `main-aside` / `aside-main`: 2/3 + 1/3 a partir de 1000 px.
   * - `auto`: quantas couberem com `minWidth` cada.
   */
  cols?: GridLayout;
  /** Largura mínima de cada coluna em `cols="auto"` (CSS, ex.: "220px"). */
  minWidth?: string;
  /** Alinha os itens no topo, em vez de esticar todos à altura da linha. */
  alignStart?: boolean;
};

/** Grade responsiva pela largura do conteúdo (container query), não da janela. */
export function Grid({ as: Tag = "div", cols = 2, minWidth, alignStart = false, className, style, children, ...rest }: GridProps) {
  return (
    <Tag
      className={cx("grid", `grid--${cols}`, alignStart && "grid--start", className)}
      style={minWidth ? { ...style, ["--grid-min" as string]: minWidth } : style}
      {...rest}
    >
      {children}
    </Tag>
  );
}

type PanelProps = Omit<HTMLAttributes<HTMLElement>, "title"> & {
  as?: "section" | "article" | "div" | "aside" | "li";
  /** O título do painel. Sem título, o painel precisa de `aria-label` se for `section`. */
  title?: ReactNode;
  titleAs?: "h2" | "h3";
  /** A explicação do painel, no ⓘ ao lado do título. */
  info?: ReactNode;
  /** O nome do ⓘ; o padrão é "Sobre <título>" quando o título é texto. */
  infoLabel?: string;
  /** Ação do painel (link, botão, filtro), à direita do título. */
  actions?: ReactNode;
  /** Recheio menor, para painéis de apoio. */
  compact?: boolean;
  /** Sem recheio no corpo: a tabela vai de borda a borda. */
  flush?: boolean;
  /** O holofote que segue o ponteiro (o padrão do vidro). */
  spotlight?: boolean;
};

/**
 * Um cartão de vidro com cabeçalho de uma linha (título, ⓘ, ação) e o corpo.
 * O título nomeia a região (`aria-labelledby`).
 */
export function Panel({
  as = "section",
  title,
  titleAs: Title = "h2",
  info,
  infoLabel,
  actions,
  compact = false,
  flush = false,
  spotlight = true,
  className,
  children,
  ...rest
}: PanelProps) {
  const titleId = useId();
  const hasHead = title !== undefined || actions !== undefined;
  const tipLabel = infoLabel ?? (typeof title === "string" ? `Sobre ${title}` : "Sobre este painel");
  return (
    <GlassCard
      as={as}
      spotlight={spotlight}
      className={cx("panel", compact && "panel--compact", flush && "panel--flush", className)}
      aria-labelledby={title !== undefined ? titleId : undefined}
      {...rest}
    >
      {hasHead && (
        <header className="panel__head">
          {title !== undefined && (
            <Title id={titleId} className="panel__title">
              {title}
            </Title>
          )}
          {info && <InfoTip label={tipLabel}>{info}</InfoTip>}
          {actions && <div className="panel__actions">{actions}</div>}
        </header>
      )}
      <div className="panel__body">{children}</div>
    </GlassCard>
  );
}

export type Stat = {
  /** Chave estável (o rótulo serve se for único). */
  id?: string;
  label: string;
  value: ReactNode;
  /** Uma linha curta sob o número (a base, o período). */
  detail?: ReactNode;
  /** A explicação do número, no ⓘ do rótulo. */
  info?: ReactNode;
  /** Ouro para dinheiro em destaque; alta/baixa só reforçam um sinal já escrito. */
  tone?: "gold" | "up" | "down";
};

type StatStripProps = {
  items: Stat[];
  /** Dentro de um painel: sem moldura nos números. */
  inset?: boolean;
  className?: string;
};

/**
 * A faixa de números lado a lado: rótulo pequeno, número grande, detalhe. É
 * uma lista de definição (`<dl>`: o leitor de tela lê rótulo e número
 * juntos), e cada número quebra para a linha de baixo quando não cabe (mínimo
 * de 150 px por número). Quem nomeia a faixa é o painel ou a seção em volta.
 */
export function StatStrip({ items, inset = false, className }: StatStripProps) {
  return (
    <dl className={cx("stat-strip", inset && "stat-strip--inset", className)}>
      {items.map((item) => (
        <div key={item.id ?? item.label} className={cx("stat", item.tone && `stat--${item.tone}`)}>
          <dt className="stat__label">
            {item.label}
            {item.info && <InfoTip label={`Sobre ${item.label}`}>{item.info}</InfoTip>}
          </dt>
          <dd className="stat__value">{item.value}</dd>
          {item.detail && <dd className="stat__detail">{item.detail}</dd>}
        </div>
      ))}
    </dl>
  );
}
