import { useId } from "react";
import type { Analysis, Mix } from "../../api/analysisTypes";
import { GlassCard } from "../../components/GlassCard";
import { PROFILE_LABEL } from "../../lib/analysisText";
import { formatPercent } from "../../lib/format";

type Profile = Analysis["profile"];

/** A barra de uma mistura: renda fixa de um lado (listrada), renda variável do outro (lisa), os dois em texto. */
function MixBar({ title, mix, empty }: { title: string; mix: Mix | null; empty: string }) {
  if (!mix) {
    return (
      <div className="mix">
        <p className="mix__title">{title}</p>
        <p className="field__hint">{empty}</p>
      </div>
    );
  }
  const fixed = formatPercent(mix.fixed_income);
  const variable = formatPercent(mix.variable);
  return (
    <div className="mix">
      <p className="mix__title">
        {title}: <strong>{PROFILE_LABEL[mix.profile]}</strong>
      </p>
      <div
        className="mix__bar"
        role="img"
        aria-label={`${title}: perfil ${PROFILE_LABEL[mix.profile].toLowerCase()}, renda fixa ${fixed}, renda variável ${variable}`}
      >
        <span className="mix__fixed" style={{ width: `${mix.fixed_income}%` }} />
        <span className="mix__variable" style={{ width: `${mix.variable}%` }} />
      </div>
      <p className="mix__legend">
        <span>
          <i className="mix__dot mix__dot--fixed" aria-hidden="true" /> Renda fixa {fixed}
        </span>
        <span>
          <i className="mix__dot mix__dot--variable" aria-hidden="true" /> Renda variável {variable}
        </span>
      </p>
    </div>
  );
}

/** Quanto a renda variável de agora está acima ou abaixo da meta, em uma frase. */
function comparison(profile: Profile): string | null {
  const { current, target } = profile;
  if (!current || !target) return null;
  const gap = current.variable - target.variable;
  if (Math.abs(gap) < 1) return "A sua carteira está no perfil que as suas metas pedem.";
  const points = Math.abs(gap).toFixed(1).replace(".", ",");
  return `A renda variável está ${points} pontos percentuais ${gap > 0 ? "acima" : "abaixo"} do que as suas metas pedem.`;
}

/**
 * Perfil de risco: onde a carteira está entre renda fixa e renda variável, o
 * rótulo que isso merece e o mesmo para as metas por classe, para comparar o
 * atual com o desejado. Os limiares são do Prumo e estão escritos aqui.
 */
export function ProfileCard({ profile }: { profile: Profile }) {
  const titleId = useId();
  const note = comparison(profile);
  const conservative = formatPercent(profile.conservative_max, 0);
  const moderate = formatPercent(profile.moderate_max, 0);

  return (
    <GlassCard as="section" className="profile" aria-labelledby={titleId}>
      <h2 id={titleId} className="section-title">
        Perfil de risco
      </h2>
      <MixBar title="Agora" mix={profile.current} empty="Ainda não há ativos com valor." />
      <MixBar
        title="Suas metas"
        mix={profile.target}
        empty="Defina as metas por classe, em Ajustes, Metas, para comparar com o que você tem."
      />
      {note && <p className="profile__note">{note}</p>}
      <p className="field__hint">
        Renda fixa são Renda Fixa e Renda Fixa Internacional; o resto é renda variável. Pelos limiares do Prumo, até{" "}
        {conservative} em renda variável é conservador, até {moderate} é moderado e acima disso é arrojado.
      </p>
    </GlassCard>
  );
}
