/**
 * O fundo vivo: manchas de luz muito desfocadas derivando devagar, com grão
 * por cima. Só CSS — com `prefers-reduced-motion` as manchas param.
 */
export function AuroraBackground() {
  return (
    <div className="aurora" aria-hidden="true">
      <div className="aurora__blob aurora__blob--indigo" />
      <div className="aurora__blob aurora__blob--teal" />
      <div className="aurora__blob aurora__blob--violet" />
      <div className="aurora__blob aurora__blob--amber" />
      <div className="aurora__grain" />
    </div>
  );
}
