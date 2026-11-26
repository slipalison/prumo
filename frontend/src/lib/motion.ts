/*
 * Os tokens de movimento do app, num lugar só (docs/layout.md, "Movimento").
 * O CSS tem os mesmos números em `styles/tokens.css` (`--dur-*`, `--ease-*`);
 * um teste confere que os dois lados não se separam.
 *
 * Regra: nada que a pessoa espera passa de ~300 ms. Ficam de fora só o que é
 * ambiente ou identidade — o fundo que deriva, o prumo que balança enquanto
 * calcula, o brilho do esqueleto, o pulso do "Ao vivo" e o lampejo do preço
 * (que diz o sentido por 1,6 s). Quem pede menos movimento no sistema recebe
 * menos: `MotionConfig reducedMotion="user"` (Providers) e a regra global de
 * `base.css`.
 */

/** Durações, em segundos (a unidade do `motion`). */
export const DURATION = {
  /** Toque e pressão: a resposta tem de parecer instantânea. */
  instant: 0.09,
  /** Hover, cor, toggletip, troca de aba. */
  fast: 0.14,
  /** Entrada e saída de tela, folha e cartão. */
  base: 0.2,
  /** Barras, roscas e linhas que crescem até o valor. */
  slow: 0.28,
} as const;

/** Saída rápida e pouso suave: a curva padrão de tudo que entra. */
export const EASE_OUT = [0.22, 1, 0.36, 1] as const;
/** Para o que sai de cena: acelera e some. */
export const EASE_IN = [0.4, 0, 1, 1] as const;

/**
 * Molas. Todas quase criticamente amortecidas (sem quicar à toa) e assentando
 * em até ~0,45 s; a de valor é a mais lenta, porque o olho acompanha o número.
 */
export const SPRING = {
  /** Botões de alternância, indicador da navegação, nota que vira. */
  snappy: { type: "spring", stiffness: 520, damping: 40 },
  /** Folhas, avisos, cartões que entram. */
  enter: { type: "spring", stiffness: 420, damping: 36 },
  /** Barras, roscas, contadores e linhas de gráfico até o valor. */
  value: { type: "spring", stiffness: 190, damping: 27 },
  /** O botão principal que puxa na direção do cursor (`useSpring`, sem `type`). */
  magnet: { stiffness: 320, damping: 24 },
} as const;

/** Intervalo entre itens de uma lista que entra em cascata. */
export const STAGGER = 0.03;

/** A entrada padrão de um bloco: aparece subindo 6 px. */
export const enterUp = {
  initial: { opacity: 0, y: 6 },
  animate: { opacity: 1, y: 0 },
  transition: { duration: DURATION.base, ease: EASE_OUT },
} as const;

/** A troca de tela: sai rápido, entra rápido (o `AnimatePresence` espera a saída). */
export const screenTransition = {
  initial: { opacity: 0, y: 6 },
  animate: { opacity: 1, y: 0, transition: { duration: DURATION.base, ease: EASE_OUT } },
  exit: { opacity: 0, y: -4, transition: { duration: DURATION.instant, ease: EASE_IN } },
} as const;
