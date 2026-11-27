// Roda antes do React para o tema salvo valer já na primeira pintura.
// Mesma regra de src/lib/theme.ts: "system" remove o atributo e deixa a media
// query mandar; sem escolha salva, o padrão é o escuro.
(function () {
  const root = document.documentElement;
  try {
    const saved = window.localStorage.getItem("prumo.theme");
    const pref = saved === "light" || saved === "system" ? saved : "dark";
    if (pref !== "system") root.dataset.theme = pref;
  } catch {
    // O localStorage pode estar fechado (modo privado, armazenamento bloqueado).
    // Não há o que avisar: sem escolha legível, vale o padrão, o escuro.
    root.dataset.theme = "dark";
  }
})();
