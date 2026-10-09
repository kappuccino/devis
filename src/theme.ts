// Thème de l'interface : clair, sombre, ou automatique (suit le système).
// Mémorisé sur ce poste ; appliqué avant l'affichage pour éviter un flash.

export type ThemeMode = "auto" | "light" | "dark";

const KEY = "ui-theme";
const media = window.matchMedia("(prefers-color-scheme: dark)");

export function getTheme(): ThemeMode {
  try {
    const v = localStorage.getItem(KEY);
    return v === "light" || v === "dark" ? v : "auto";
  } catch {
    return "auto";
  }
}

/** Ajoute ou retire la classe `app-dark` (utilisée par les couleurs de l'appli et par PrimeVue). */
export function applyTheme(mode: ThemeMode = getTheme()) {
  const dark = mode === "dark" || (mode === "auto" && media.matches);
  document.documentElement.classList.toggle("app-dark", dark);
}

export function setTheme(mode: ThemeMode) {
  try {
    localStorage.setItem(KEY, mode);
  } catch {
    // Stockage indisponible : le choix vaut pour cette session.
  }
  applyTheme(mode);
}

// Mode automatique : suit les changements du système en direct.
media.addEventListener("change", () => applyTheme());
