import { useState, type ComponentType } from "react";
import { iconUrl } from "../api/icons";
import { useIconIndex } from "../api/iconsQueries";
import type { AssetClass } from "../api/types";
import { classColor } from "../lib/assetClasses";
import { iconClass, iconKind, initialsOf, type IconKind } from "../lib/assetIcon";
import { cx } from "../lib/cx";
import { GlyphBitcoin, GlyphCrypto, GlyphTreasury } from "./assetGlyphs";

export type AssetIconProps = {
  /** O ticker (ou a chave do título do Tesouro); `null` na renda fixa por valor. */
  symbol?: string | null;
  /** O nome do ativo: dá as iniciais quando não há ticker. */
  name?: string | null;
  /** A classe, onde a tela a sabe; sem ela, o palpite pelo ticker. */
  assetClass?: AssetClass | null;
  treasury?: boolean;
  size?: "sm" | "md" | "lg";
  className?: string;
};

const GLYPHS: Record<Exclude<IconKind, "initials">, ComponentType<{ className?: string }>> = {
  treasury: GlyphTreasury,
  bitcoin: GlyphBitcoin,
  crypto: GlyphCrypto,
};

/**
 * O ícone de um ativo: o logo da empresa ou do fundo quando o servidor tem
 * (vem do domínio do Prumo, nunca de terceiro), e, sem ele, o ícone próprio do
 * Tesouro e da cripto ou as iniciais do ticker na cor da classe.
 *
 * É enfeite: o nome do ativo sempre está ao lado, então a imagem leva `alt`
 * vazio e o conjunto `aria-hidden`. Não pisca: o círculo de iniciais está
 * sempre por baixo, e o logo só aparece, por cima, depois de carregar. Se a
 * imagem falha, ela sai e as iniciais ficam.
 */
export function AssetIcon({ symbol, name, assetClass, treasury, size = "md", className }: AssetIconProps) {
  const versions = useIconIndex().data;
  const subject = { symbol, assetClass, treasury };
  const kind = iconKind(subject);
  const tone = iconClass(subject);
  // O título do Tesouro não tem ticker: nunca procura logo.
  const key = kind === "treasury" ? undefined : symbol?.trim().toUpperCase();
  const version = key ? versions?.get(key) : undefined;
  const src = key && version ? iconUrl(key, version) : null;

  // Por URL: um logo novo (outra versão) tem nova chance depois de um erro.
  const [failed, setFailed] = useState<string | null>(null);
  const [loaded, setLoaded] = useState<string | null>(null);

  const Glyph = kind === "initials" ? null : GLYPHS[kind];
  return (
    <span
      className={cx("asset-icon", `asset-icon--${size}`, className)}
      style={{ ["--icon-bg" as string]: tone ? classColor(tone) : "var(--text-muted)" }}
      data-kind={kind}
      aria-hidden="true"
    >
      {Glyph ? (
        <Glyph className="asset-icon__glyph" />
      ) : (
        <span className="asset-icon__initials">{initialsOf(symbol, name)}</span>
      )}
      {src !== null && failed !== src && (
        <img
          className="asset-icon__img"
          src={src}
          alt=""
          loading="lazy"
          decoding="async"
          draggable={false}
          data-loaded={loaded === src ? "" : undefined}
          onLoad={() => setLoaded(src)}
          onError={() => setFailed(src)}
        />
      )}
    </span>
  );
}
