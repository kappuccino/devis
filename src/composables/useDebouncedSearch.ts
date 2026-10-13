import { onScopeDispose, ref, watch, type Ref } from "vue";

/**
 * Texte de recherche appliqué après une pause de frappe, pour ne pas refiltrer de longues listes
 * à chaque touche. Un champ vidé s'applique tout de suite.
 */
export function useDebouncedSearch(source: Ref<string>, delay = 250): Readonly<Ref<string>> {
  const debounced = ref(source.value);
  let timer: ReturnType<typeof setTimeout> | undefined;
  watch(source, (value) => {
    clearTimeout(timer);
    if (!value.trim()) debounced.value = value;
    else timer = setTimeout(() => (debounced.value = value), delay);
  });
  onScopeDispose(() => clearTimeout(timer));
  return debounced;
}
