import { ScoreBadge } from "./ScoreBadge";

type ScoreSliderProps = {
  id: string;
  value: number;
  onChange: (value: number) => void;
  label?: string;
  hint?: string;
};

/** Nota manual de 0 a 10, para cripto, renda fixa ou quem prefere não usar o checklist. */
export function ScoreSlider({ id, value, onChange, label = "Nota de confiança (0 a 10)", hint }: Readonly<ScoreSliderProps>) {
  return (
    <div className="score-slider">
      <div className="score-slider__head">
        <label htmlFor={id} className="field__label">
          {label}
        </label>
        <ScoreBadge score={value} size="lg" />
      </div>
      <input
        id={id}
        className="range"
        type="range"
        min={0}
        max={10}
        step={1}
        value={value}
        style={{ ["--fill" as string]: `${value * 10}%` }}
        onChange={(event) => onChange(Number(event.target.value))}
      />
      {hint && <p className="field__hint">{hint}</p>}
    </div>
  );
}
