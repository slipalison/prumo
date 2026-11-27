import { motion } from "motion/react";
import { useId, type ReactNode } from "react";
import { cx } from "../../lib/cx";
import "./nativeTags.css";

export type SegmentedOption<T extends string> = { id: T; label: ReactNode };

type SegmentedProps<T extends string> = {
  /** O nome do grupo para o leitor de tela ("Data do provento"). */
  label: string;
  options: readonly SegmentedOption<T>[];
  value: T;
  onChange: (value: T) => void;
};

/**
 * A escolha entre poucas opções (o segmentado do app, `.segmented`): um grupo
 * de botões com `aria-pressed`, e o fundo dourado desliza até a escolhida.
 */
export function Segmented<T extends string>({ label, options, value, onChange }: Readonly<SegmentedProps<T>>) {
  const knob = useId();
  return (
    <fieldset className="segmented income-history__segmented" aria-label={label}>
      {options.map((option) => (
        <button
          key={option.id}
          type="button"
          className={cx("segmented__option", value === option.id && "is-active")}
          aria-pressed={value === option.id}
          onClick={() => onChange(option.id)}
        >
          {value === option.id && <motion.span layoutId={knob} className="segmented__knob" />}
          <span className="segmented__label">{option.label}</span>
        </button>
      ))}
    </fieldset>
  );
}
