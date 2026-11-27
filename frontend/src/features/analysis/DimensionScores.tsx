import { motion } from "motion/react";
import type { Dimension, DimensionId } from "../../api/analysisTypes";
import { StatusPill } from "../../components/DataTable";
import { DIMENSION_TEXT, dimensionAnchor, noDataText, scorePercent, scoreText } from "../../lib/analysisText";
import { enterUp, SPRING, STAGGER } from "../../lib/motion";

/** Leva até o painel da dimensão e põe o foco nele (o teclado continua de lá). */
function jumpTo(id: DimensionId) {
  const target = document.getElementById(dimensionAnchor(id));
  if (!target) return;
  const reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
  target.scrollIntoView?.({ behavior: reduced ? "auto" : "smooth", block: "start" });
  target.focus({ preventScroll: true });
}

/** A pílula ao lado do percentual: quantas regras a ver, tudo ok ou nada (sem regra medida). */
function ScorePill({ warnings, measured }: Readonly<{ warnings: number; measured: number }>) {
  if (warnings > 0) return <StatusPill status="attention">{warnings} a ver</StatusPill>;
  if (measured > 0) return <StatusPill status="ok">Tudo ok</StatusPill>;
  return null;
}

/**
 * As quatro pontuações lado a lado, no topo: o percentual grande, a barra e o
 * número exato por trás ("3 de 9 regras ok"). Cada cartão leva até a tabela
 * da dimensão.
 */
export function DimensionScores({ dimensions }: Readonly<{ dimensions: Dimension[] }>) {
  return (
    <ul className="dim-scores" aria-label="Pontuação por dimensão">
      {dimensions.map((dimension, index) => {
        const { title } = DIMENSION_TEXT[dimension.id];
        const { score } = dimension;
        const percent = scorePercent(score);
        const missing = noDataText(score);
        const warnings = score.measured - score.ok;
        return (
          <motion.li
            key={dimension.id}
            className="glass dim-score"
            initial={enterUp.initial}
            animate={enterUp.animate}
            transition={{ ...enterUp.transition, delay: index * STAGGER }}
          >
            <p className="dim-score__title">{title}</p>
            <div className="dim-score__head">
              <p className="dim-score__percent">{percent ?? "Sem dado"}</p>
              <ScorePill warnings={warnings} measured={score.measured} />
            </div>
            {score.percent !== null && (
              <div // NOSONAR: o <meter> nativo desenha a própria barra e não mostra filhos; aqui o preenchimento é animado (motion)
                className="dim-score__meter"
                role="meter"
                aria-label={`${title}: ${scoreText(score)}`}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={score.percent}
              >
                <motion.span
                  className="dim-score__fill"
                  initial={{ width: "0%" }}
                  animate={{ width: `${score.percent}%` }}
                  transition={SPRING.value}
                />
              </div>
            )}
            <p className="dim-score__detail">
              {score.measured > 0 ? scoreText(score) : "Nenhuma regra com dado ainda"}
              {missing && ` · ${missing}`}
            </p>
            <button
              type="button"
              className="dim-score__jump"
              aria-label={`Ver as regras de ${title}`}
              onClick={() => jumpTo(dimension.id)}
            />
          </motion.li>
        );
      })}
    </ul>
  );
}
