<script setup lang="ts">
import { nextTick, watch } from "vue";
import type { SearchResult } from "../../docs/search";
import { dragOnMove, prepareDrag } from "../../docs/drag";
import { dirName, fileName, formatFileDate } from "../../docs/files";

const props = defineProps<{ results: SearchResult[]; selected: SearchResult | null; query: string }>();
const emit = defineEmits<{ select: [r: SearchResult] }>();

/** Texte découpé autour de la partie surlignée. */
const split = (text: string, h: { start: number; end: number } | null) =>
  h ? [text.slice(0, h.start), text.slice(h.start, h.end), text.slice(h.end)] : [text, "", ""];

const MATCH_LABEL: Partial<Record<SearchResult["match"], string>> = { prefix: "début de réf.", partial: "approximatif" };

const where = (r: SearchResult) =>
  r.kind === "image" ? "image" : r.pageNum == null ? `fichier entier · ${r.pageCount} p.` : `page ${r.pageNum}`;

const isSelected = (r: SearchResult) => props.selected?.docPath === r.docPath && props.selected?.pageNum === r.pageNum;

watch(
  () => props.selected,
  async () => {
    await nextTick();
    document.querySelector(".doc-result.selected")?.scrollIntoView({ block: "nearest" });
  },
);
</script>

<template>
  <ul class="doc-results">
    <li
      v-for="r in results"
      :key="`${r.docPath}#${r.pageNum}`"
      class="doc-result"
      :class="{ selected: isSelected(r) }"
      :title="`Cliquer pour l’aperçu · glisser vers une autre application pour déposer ${r.pageNum == null ? 'le fichier' : 'la page en PDF'}`"
      @click="emit('select', r)"
      @mouseenter="prepareDrag(r, query)"
      @mousedown="dragOnMove($event, r, query)"
    >
      <div class="head">
        <span class="name">
          {{ split(fileName(r.docPath), r.nameHighlight)[0] }}<mark>{{ split(fileName(r.docPath), r.nameHighlight)[1] }}</mark>{{ split(fileName(r.docPath), r.nameHighlight)[2] }}
        </span>
        <span class="where">{{ where(r) }}</span>
        <span class="date" title="Date de création du fichier">{{ formatFileDate(r.createdAt) }}</span>
      </div>
      <div class="path">{{ dirName(r.docPath) }}</div>
      <div class="snippet">
        <span v-if="r.nameMatch" class="badge name">nom du fichier</span>
        <template v-if="r.snippet">
          {{ split(r.snippet, r.highlight)[0] }}<mark>{{ split(r.snippet, r.highlight)[1] }}</mark>{{ split(r.snippet, r.highlight)[2] }}
        </template>
        <span v-if="MATCH_LABEL[r.match]" class="badge" :class="r.match">{{ MATCH_LABEL[r.match] }}</span>
      </div>
    </li>
  </ul>
</template>

<style scoped>
.doc-results {
  list-style: none;
  margin: 0;
  padding: 0;
}

.doc-result {
  padding: 0.6rem 0.75rem;
  border-bottom: 1px solid var(--app-border);
  cursor: grab;
  user-select: none;
}

.doc-result:hover {
  background: var(--app-bg);
}

.doc-result.selected {
  background: color-mix(in srgb, var(--app-accent) 10%, transparent);
  box-shadow: inset 3px 0 0 var(--app-accent);
}

.head {
  display: flex;
  gap: 0.6rem;
  align-items: baseline;
}

.name {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  min-width: 0;
}

.where,
.date {
  flex: none;
  font-size: 0.85em;
  color: var(--app-muted);
}

/* Dossier tronqué par la gauche : on garde la fin du chemin, la plus parlante. */
.path {
  font-size: 0.8em;
  color: var(--app-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  direction: rtl;
  text-align: left;
}

.snippet {
  margin-top: 0.25rem;
  font-size: 0.88em;
  line-height: 1.4;
}

mark {
  background: color-mix(in srgb, var(--app-accent) 22%, transparent);
  color: inherit;
  border-radius: 2px;
  padding: 0 1px;
}

.badge {
  display: inline-block;
  font-size: 0.75em;
  padding: 0 0.4rem;
  margin: 0 0.3rem 0 0;
  border-radius: 999px;
  background: var(--app-bg);
  border: 1px solid var(--app-border);
  color: var(--app-muted);
}

.badge.name {
  border-color: color-mix(in srgb, var(--app-accent) 50%, transparent);
  color: var(--app-accent);
}

.badge.partial {
  color: var(--app-warn);
}
</style>
