import { motion } from "motion/react";
import type { Answer } from "../lib/score";
import { cx } from "../lib/cx";
import { SPRING } from "../lib/motion";

type AnswerToggleProps = {
  questionId: string;
  prompt: string;
  value: Answer;
  onChange: (value: Answer) => void;
  disabled?: boolean;
};

const OPTIONS = [
  { answer: true, label: "Sim", tone: "yes" },
  { answer: false, label: "Não", tone: "no" },
] as const;

/**
 * Sim / Não em pílulas. Tocar na opção já marcada desfaz (volta para sem
 * resposta, que não conta na nota). O destaque escorrega entre as duas.
 */
export function AnswerToggle({ questionId, prompt, value, onChange, disabled }: Readonly<AnswerToggleProps>) {
  return (
    <fieldset className="answer" aria-label={prompt}>
      <p className="answer__prompt">{prompt}</p>
      <div className="answer__pills">
        {OPTIONS.map((option) => {
          const active = value === option.answer;
          return (
            <button
              key={option.label}
              type="button"
              className={cx("answer__pill", `answer__pill--${option.tone}`, active && "is-active")}
              aria-pressed={active}
              disabled={disabled}
              onClick={() => onChange(active ? null : option.answer)}
            >
              {active && (
                <motion.span
                  layoutId={`answer-knob-${questionId}`}
                  className="answer__knob"
                  transition={SPRING.snappy}
                />
              )}
              <span className="answer__label">{option.label}</span>
            </button>
          );
        })}
      </div>
    </fieldset>
  );
}
