<script setup lang="ts">
import { ref, watch } from "vue";
import { useRouter } from "vue-router";
import InputText from "primevue/inputtext";
import Button from "primevue/button";
import ProgressBar from "primevue/progressbar";
import DocResultList from "../components/docs/DocResultList.vue";
import DocPagePreview from "../components/docs/DocPagePreview.vue";
import { searchDocs, type SearchResult } from "../docs/search";
import { docs, indexing } from "../docs/store";
import { fileName } from "../docs/files";
import { errorMessage } from "../format";

const router = useRouter();

// La saisie et le résultat choisi sont gardés en quittant l'écran (on y revient souvent).
const query = ref(lastQuery);
const results = ref<SearchResult[]>([]);
const selected = ref<SearchResult | null>(null);
const searchMs = ref(0);
const message = ref("");

async function runSearch() {
  const q = query.value;
  lastQuery = q;
  try {
    const t0 = performance.now();
    const res = await searchDocs(q);
    if (q !== query.value) return; // une saisie plus récente a pris le relais
    searchMs.value = performance.now() - t0;
    results.value = res;
    // Garde le résultat choisi s'il est toujours là, sinon le premier.
    const keep = res.find((r) => r.docPath === selected.value?.docPath && r.pageNum === selected.value?.pageNum);
    selected.value = keep ?? res[0] ?? null;
  } catch (e) {
    message.value = `Recherche impossible : ${errorMessage(e)}`;
  }
}

let timer: ReturnType<typeof setTimeout> | undefined;
watch(query, () => {
  clearTimeout(timer);
  timer = setTimeout(runSearch, 200);
});
// Index mis à jour : on relance la recherche affichée.
watch(
  () => docs.version,
  () => query.value && runSearch(),
);
if (query.value) runSearch();

function onKey(e: KeyboardEvent) {
  if (!results.value.length || !["ArrowDown", "ArrowUp"].includes(e.key)) return;
  e.preventDefault();
  const i = results.value.indexOf(selected.value!);
  const next = e.key === "ArrowDown" ? Math.min(i + 1, results.value.length - 1) : Math.max(i - 1, 0);
  selected.value = results.value[next];
}

const countLabel = (n: number) => `${n} résultat${n > 1 ? "s" : ""}`;
</script>

<script lang="ts">
let lastQuery = "";
</script>

<template>
  <div class="documentation" @keydown="onKey">
    <div class="split">
      <!-- Recherche et résultats -->
      <aside class="list-pane">
        <div class="search">
          <i class="pi pi-search" />
          <InputText
            v-model="query"
            type="search"
            placeholder="Référence produit (ex. 0540010R13, AB-1234-X)…"
            autofocus
            spellcheck="false"
            autocomplete="off"
            fluid
          />
        </div>
        <div class="list">
          <div v-if="!docs.folder" class="empty">
            <p>Aucun dossier de documentation n'est choisi.</p>
            <Button label="Choisir le dossier dans les Réglages" icon="pi pi-cog" text @click="router.push('/reglages/documentation')" />
          </div>
          <p v-else-if="query.trim().length < 3" class="empty muted">Tapez une référence (3 caractères minimum).</p>
          <p v-else-if="!results.length" class="empty muted">Aucun résultat.</p>
          <template v-else>
            <p class="count muted">{{ countLabel(results.length) }} · {{ searchMs.toFixed(0) }} ms</p>
            <DocResultList :results="results" :selected="selected" :query="query" @select="selected = $event" />
          </template>
        </div>
      </aside>

      <!-- Aperçu -->
      <DocPagePreview :result="selected" :query="query" class="preview-pane" @notify="message = $event" />
    </div>

    <!-- Barre d'état : indexation en cours, dernier message, fichiers non indexés -->
    <footer class="statusbar">
      <template v-if="indexing && docs.progress">
        <span class="ellipsis">
          Indexation {{ docs.progress.index + 1 }}/{{ docs.progress.total || "…" }} · {{ fileName(docs.progress.file || "") }}
          <template v-if="docs.progress.pageCount"> · page {{ docs.progress.pageNum }}/{{ docs.progress.pageCount }}</template>
        </span>
        <ProgressBar
          :value="docs.progress.total ? Math.round((100 * docs.progress.index) / docs.progress.total) : 0"
          :show-value="false"
          class="progress"
        />
      </template>
      <span v-else class="ellipsis">
        {{ message || docs.notice || `${docs.stats.docs} PDF · ${docs.stats.images} images · ${docs.stats.pages} pages indexées` }}
      </span>
      <span class="spacer" />
      <Button
        v-if="!indexing && docs.errors.length"
        :label="`${docs.errors.length} fichier(s) non indexé(s)`"
        icon="pi pi-exclamation-triangle"
        severity="danger"
        text
        size="small"
        @click="router.push('/reglages/documentation')"
      />
      <span v-if="docs.info && docs.info.refMode !== 'trigram'" class="warn">Mode LIKE (trigram absent)</span>
    </footer>
  </div>
</template>

<style scoped>
.documentation {
  height: 100%;
  display: flex;
  flex-direction: column;
}

.split {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(320px, 42%) 1fr;
}

.list-pane {
  display: flex;
  flex-direction: column;
  min-height: 0;
  border-right: 1px solid var(--app-border);
  background: var(--app-surface);
}

.search {
  position: relative;
  padding: 0.75rem;
  border-bottom: 1px solid var(--app-border);
}

.search i {
  position: absolute;
  left: 1.5rem;
  top: 50%;
  transform: translateY(-50%);
  color: var(--app-muted);
  pointer-events: none;
}

.search :deep(input) {
  padding-left: 2.2rem;
}

.list {
  flex: 1;
  overflow: auto;
}

.count {
  margin: 0;
  padding: 0.5rem 0.75rem;
  font-size: 0.85em;
  border-bottom: 1px solid var(--app-border);
}

.empty {
  padding: 1.5rem 1rem;
  margin: 0;
  text-align: center;
}

.preview-pane {
  min-height: 0;
}

.statusbar {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.3rem 0.9rem;
  min-height: 34px;
  border-top: 1px solid var(--app-border);
  background: var(--app-surface);
  font-size: 0.85em;
  color: var(--app-muted);
}

.progress {
  width: 180px;
  height: 6px;
  flex: none;
}

.ellipsis {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.spacer {
  flex: 1;
}
</style>
