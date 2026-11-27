import { useId, useState, type ReactNode } from "react";
import { useAllocationSettings, useSaveAllocationSettings } from "../../api/allocation";
import type { AllocationSettings } from "../../api/allocationTypes";
import { errorMessage } from "../../api/client";
import { Panel } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { useToast } from "../../components/toastContext";
import { formatPercent } from "../../lib/format";

/** Onde o controle começa quando a pessoa liga o limite pela primeira vez. */
const DEFAULT_LIMIT = 20;

/**
 * O limite de concentração: guarda-corpo opcional, por pessoa. Nenhum ativo
 * passa da fatia escolhida da carteira depois do aporte; o que passaria vai
 * para os outros ativos da classe (docs/alocacao.md).
 */
export function ConcentrationLimitCard() {
  const settings = useAllocationSettings();
  let content: ReactNode;
  if (settings.isError) {
    content = (
      <p className="form-error" role="alert">
        {errorMessage(settings.error)}
      </p>
    );
  } else if (settings.data) {
    // A chave recomeça o formulário quando o valor salvo muda.
    content = <LimitForm key={String(settings.data.max_asset_percent)} saved={settings.data} />;
  } else {
    content = <p className="field__hint">Carregando…</p>;
  }

  return (
    <Panel
      title="Limite por ativo"
      className="limit-card"
      infoLabel="Como funciona o limite por ativo"
      info="Opcional. Nenhum ativo passa desta fatia da carteira depois do aporte; o que passaria vai para os outros ativos da mesma classe. O motor nunca vende: quem já está acima só deixa de receber."
    >
      {content}
    </Panel>
  );
}

function LimitForm({ saved }: Readonly<{ saved: AllocationSettings }>) {
  const ids = { toggle: useId(), range: useId() };
  const [enabled, setEnabled] = useState(saved.max_asset_percent !== null);
  const [percent, setPercent] = useState(saved.max_asset_percent ?? DEFAULT_LIMIT);
  const save = useSaveAllocationSettings();
  const toast = useToast();

  const next = enabled ? percent : null;
  const dirty = next !== saved.max_asset_percent;

  const submit = () => {
    save.mutate(
      { max_asset_percent: next },
      {
        onSuccess: (result) =>
          toast({
            tone: "success",
            message:
              result.max_asset_percent === null
                ? "Limite desligado."
                : `Limite salvo: nenhum ativo passa de ${formatPercent(result.max_asset_percent, 0)} da carteira.`,
          }),
      },
    );
  };

  return (
    <>
      <div className="switch-row">
        <label htmlFor={ids.toggle} className="switch-row__label">
          Limitar a concentração{" "}
          <span className="field__hint">
            {saved.max_asset_percent === null
              ? "Desligado: cada ativo recebe pela nota, sem teto."
              : `Ligado em ${formatPercent(saved.max_asset_percent, 0)} da carteira.`}
          </span>
        </label>
        <input
          id={ids.toggle}
          type="checkbox"
          role="switch"
          className="switch"
          checked={enabled}
          onChange={(event) => setEnabled(event.target.checked)}
        />
      </div>
      {enabled && (
        <div className="goal-row">
          <div className="goal-row__head">
            <label htmlFor={ids.range} className="goal-row__label">
              Máximo por ativo
            </label>
            <span className="goal-row__value">{formatPercent(percent, 0)}</span>
          </div>
          <input
            id={ids.range}
            className="range"
            type="range"
            min={1}
            max={100}
            step={1}
            value={percent}
            aria-valuetext={`${formatPercent(percent, 0)} da carteira`}
            style={{ ["--fill" as string]: `${percent}%` }}
            onChange={(event) => setPercent(Number(event.target.value))}
          />
        </div>
      )}
      {save.isError && (
        <p className="form-error" role="alert">
          {errorMessage(save.error)}
        </p>
      )}
      <MagneticButton disabled={!dirty} loading={save.isPending} onClick={submit} className="limit-card__save">
        Salvar limite
      </MagneticButton>
    </>
  );
}
