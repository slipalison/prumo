import { usePrivacy } from "../lib/privacy";
import { IconEye, IconEyeOff } from "./icons";

/**
 * O olho: oculta ou mostra os valores em reais e as quantidades em todas as
 * telas. É um botão de alternância (`aria-pressed`), e o nome diz o que ele faz
 * no próximo toque, sem repetir nenhum valor.
 */
export function PrivacyToggle() {
  const { hidden, toggle } = usePrivacy();
  const Icon = hidden ? IconEyeOff : IconEye;
  return (
    <button
      type="button"
      className="theme-toggle privacy-toggle"
      onClick={toggle}
      aria-pressed={hidden}
      aria-label="Ocultar valores"
    >
      <span className="theme-toggle__icon">
        <Icon />
      </span>
      <span className="theme-toggle__label" aria-hidden="true">
        {hidden ? "Valores ocultos" : "Ocultar valores"}
      </span>
    </button>
  );
}
