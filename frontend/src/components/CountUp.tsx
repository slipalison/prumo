import { animate, useMotionValue, useMotionValueEvent, useReducedMotion } from "motion/react";
import { useEffect, useState } from "react";
import { cx } from "../lib/cx";
import { useMoney } from "../lib/privacy";
import { SPRING } from "../lib/motion";

type CountUpProps = {
  value: number;
  format?: (value: number) => string;
  className?: string;
};

/**
 * Número que conta até o valor com mola sempre que ele muda. Enquanto conta, o
 * leitor de tela ouve só o valor final — os intermediários ficam escondidos.
 * Com os valores ocultos, o formato padrão devolve a máscara: não há número
 * para contar nem para o leitor de tela ler.
 */
export function CountUp({ value, format, className }: Readonly<CountUpProps>) {
  const reduce = useReducedMotion();
  const money = useMoney();
  const motionValue = useMotionValue(reduce ? value : 0);
  // O estado é o número, não o texto: formatar na hora de desenhar mantém o
  // texto certo quando `format` ou a máscara mudam sem o número mudar.
  const [current, setCurrent] = useState(reduce ? value : 0);
  const show = format ?? money.brl;

  useMotionValueEvent(motionValue, "change", setCurrent);

  useEffect(() => {
    if (reduce) {
      motionValue.set(value);
      setCurrent(value);
      return undefined;
    }
    const controls = animate(motionValue, value, { ...SPRING.value, restDelta: 0.001 });
    return () => controls.stop();
  }, [value, reduce, motionValue]);

  const display = show(current);
  const final = show(value);
  const settled = display === final;

  return (
    <span className={cx("count-up", className)}>
      <span aria-hidden={settled ? undefined : true}>{display}</span>
      {!settled && <span className="sr-only">{final}</span>}
    </span>
  );
}
