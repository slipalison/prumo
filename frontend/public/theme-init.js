// Roda antes do React para o tema salvo valer já na primeira pintura.
// Mesma regra de src/lib/theme.ts: "system" remove o atributo e deixa a media
// query mandar; sem escolha salva, o padrão é o escuro.
(function () {
  try {
    var saved = window.localStorage.getItem("prumo.theme");
    var pref = saved === "light" || saved === "system" ? saved : "dark";
    if (pref !== "system") document.documentElement.setAttribute("data-theme", pref);
  } catch (e) {
    document.documentElement.setAttribute("data-theme", "dark");
  }
})();
