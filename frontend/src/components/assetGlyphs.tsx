import type { SVGProps } from "react";
import { Svg } from "./icons";

type GlyphProps = Readonly<SVGProps<SVGSVGElement>>;

/*
 * Desenhos próprios do Prumo para o que não tem logo de empresa. Nenhum copia
 * marca de terceiro: o do Tesouro é um frontão com colunas (qualquer prédio
 * público), o do bitcoin é um "B" com dois traços, e o de cripto genérica é uma
 * moeda com um losango. O nome da marca vai como texto ao lado, nunca no
 * desenho.
 */

export function GlyphTreasury(props: GlyphProps) {
  return (
    <Svg {...props}>
      <path d="M3 9.5 12 4l9 5.5" />
      <path d="M5.5 11v6M10 11v6M14 11v6M18.5 11v6" />
      <path d="M3.5 20h17" />
    </Svg>
  );
}

export function GlyphBitcoin(props: GlyphProps) {
  return (
    <Svg {...props}>
      <path d="M9 6h4.5a3 3 0 0 1 0 6H9M9 12h5.3a3 3 0 0 1 0 6H9M9 6v12" />
      <path d="M11 3.8V6M14 3.8V6M11 18v2.2M14 18v2.2" />
    </Svg>
  );
}

export function GlyphCrypto(props: GlyphProps) {
  return (
    <Svg {...props}>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M12 7.5 16.5 12 12 16.5 7.5 12z" />
    </Svg>
  );
}
