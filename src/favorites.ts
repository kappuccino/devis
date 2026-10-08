import type { Settings } from "./api";

/** Réglage : codes des listes de prix favorites, séparés par des virgules. */
export const FAVORITE_LISTS_KEY = "favorite_price_lists";

export const favoriteLists = (settings: Settings): string[] =>
  (settings[FAVORITE_LISTS_KEY] ?? "").split(",").filter(Boolean);
