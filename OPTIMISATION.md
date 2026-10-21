# Optimisations — éditeur de devis

Remontée de Julie (octobre 2026) : sur un très gros devis (≈ 400 lignes), la saisie devient lente
dès ≈ 200 lignes.

## Diagnostic

Les calculs ne sont pas en cause : totaux et sous-totaux sont en O(n) et légers, et les nombres sont
formatés avec des `Intl.NumberFormat` créés une seule fois (`src/format.ts`). Le problème vient du
rendu et des surveillances.

- **Tout le tableau redessiné à chaque frappe.** Avant la 0.2.12, les lignes étaient dans le gabarit
  de `QuoteEditView.vue`. Une touche dans un champ relançait le rendu de toutes les lignes. Chaque
  ligne contient :
  - 2 `AutoComplete` PrimeVue (ENEDIS, référence) ;
  - 4 `InputText` ;
  - 3 à 4 `v-tooltip` ;
  - des fonctions appelées pour la ligne (`isBelowThreshold` ×3, `lpnTooltip`, `isEntryLine`…).
- **Ordre de tabulation recalculé après chaque rendu.** `onUpdated(updateTabOrder)` parcourait toutes
  les cellules du tableau pour reposer les `tabindex`, à chaque touche.
- **Surveillance du brouillon en profondeur.** `watch([... lines ...], …, { deep: true })` parcourt
  tous les champs de toutes les lignes à chaque modification. Seule l'écriture du brouillon attend
  300 ms ; le parcours, lui, a lieu à chaque touche.
- **Composants lourds montés en permanence.** On a 800 `AutoComplete` pour 400 lignes, plus les
  infobulles. Cela ralentit l'ouverture du devis et les rendus.
- **Propositions de recherche partagées.** `productSuggestions` et `enedisSuggestions` étaient
  passés à toutes les lignes : chaque recherche les redessinait toutes.

## Fait (0.2.12)

- [x] **Une ligne = un composant** (`src/components/QuoteLineRow.vue`). Une frappe ne redessine que
  sa ligne.
  - Les actions de l'éditeur sont fournies une fois par `provide` (`LINE_ROW_ACTIONS`,
    `src/editLine.ts`) : des fonctions stables, qui ne provoquent aucun nouveau rendu.
  - Les propositions de recherche ne sont passées qu'à la ligne où l'on cherche (`NO_HITS` pour les
    autres).
  - Les styles du tableau restent dans `QuoteEditView.vue`, en `.lines :deep(...)`. Seul `.opt` est
    dans le composant ligne, car la liste déroulante est affichée hors du tableau.
- [x] **Ordre de tabulation posé à l'appui sur Tab** (écoute de `keydown` en phase de capture), et
  non plus après chaque rendu.

Restent à faire mesurer sur le poste de Julie.

## Pistes suivantes, par ordre d'intérêt

1. **Mesurer d'abord.** Créer un devis de test de 400 lignes et profiler une frappe (outils de
   développement de WebView2 / Safari). On saura la part du rendu, des surveillances et du reste, et
   on pourra comparer après chaque changement.
2. **Alléger la surveillance du brouillon.** Remplacer le `watch` profond sur `lines` par un compteur
   de modifications, incrémenté par les actions de saisie. Autre option : surveiller une signature
   réduite aux champs qui comptent, comme `snapshot()`. Le brouillon reste écrit 300 ms après la
   dernière frappe.
3. **Monter les champs de recherche au clic.** Une ligne déjà remplie affiche sa référence et son
   code ENEDIS dans un champ simple ; l'`AutoComplete` n'est monté qu'au focus. On passe de
   800 composants lourds à 2.
   - Gain aussi à l'ouverture des gros devis.
   - Attention au passage du clic au champ : le curseur doit arriver dans l'`AutoComplete` monté.
4. **Infobulles au survol seulement.** Garder une seule infobulle partagée, ou n'activer le
   `v-tooltip` qu'au survol, au lieu de 3 à 4 directives par ligne.
5. **Rendu du parent plus léger.** Le parent est encore redessiné à chaque quantité ou prix
   (totaux, `subtotalAmounts`, `cleanupCount`, `allLocked`…). Le coût se limite à comparer les
   propriétés de 400 composants, mais on peut :
   - isoler le panneau des totaux dans son propre composant ;
   - ne passer `subtotal` qu'aux lignes de sous-total (c'est déjà le cas).
6. **En dernier recours : virtualisation**, c'est-à-dire n'afficher que les lignes visibles. C'est
   le plus efficace, mais cela complique la tabulation (`gridNav`), le glisser-déposer, la sélection
   à la souris, la mise au point par `getElementById` et le plein écran. Ce n'est pas nécessaire pour
   400 lignes si les points 2 à 4 suffisent.
