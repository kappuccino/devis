<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import Button from "primevue/button";
import { api } from "../../api";
import { exportFileName } from "../../docs/core/index.js";
import type { SearchResult } from "../../docs/search";
import { dragOnMove, prepareDrag } from "../../docs/drag";
import { copyFile, dirName, fileManager, fileName, formatFileDate } from "../../docs/files";
import { imageUrl, pageBytes, renderPage } from "../../docs/pages";

const props = defineProps<{ result: SearchResult | null; query: string }>();
const emit = defineEmits<{ notify: [message: string] }>();

const canvas = ref<HTMLCanvasElement>();
const container = ref<HTMLElement>();
const error = ref("");
const imgSrc = ref("");
let width = 0;

const isImage = () => props.result?.kind === "image";
const isWhole = () => props.result?.pageNum == null;

async function draw() {
  const r = props.result;
  if (!r) return;
  error.value = "";
  if (isImage()) {
    imgSrc.value = "";
    try {
      const url = await imageUrl(r.docPath);
      if (props.result?.docPath === r.docPath) imgSrc.value = url;
    } catch (e) {
      error.value = `Aperçu impossible : ${e instanceof Error ? e.message : String(e)}`;
    }
    return;
  }
  if (!canvas.value || !width) return;
  try {
    // Fichier entier (PDF trouvé par son nom) : aperçu de la première page.
    await renderPage(canvas.value, r.docPath, r.pageNum ?? 1, width);
  } catch (e) {
    if ((e as { name?: string })?.name !== "RenderingCancelledException") {
      error.value = `Aperçu impossible : ${e instanceof Error ? e.message : String(e)}`;
    }
  }
}

async function exportCurrent() {
  const r = props.result;
  if (!r) return;
  try {
    if (r.pageNum == null) {
      // Image ou PDF trouvé par son nom : copie du fichier d'origine.
      const ext = r.docPath.split(".").pop() ?? "";
      const target = await save({ defaultPath: fileName(r.docPath), filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
      if (!target) return;
      await copyFile(r.docPath, target);
      emit("notify", `Fichier exporté : ${target}`);
      return;
    }
    const target = await save({
      defaultPath: exportFileName(r.docPath, r.pageNum, props.query),
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!target) return;
    await api.saveFile(target, await pageBytes(r.docPath, r.pageNum));
    emit("notify", `Page exportée : ${target}`);
  } catch (e) {
    emit("notify", `Export impossible : ${e instanceof Error ? e.message : String(e)}`);
  }
}

let observer: ResizeObserver | undefined;
onMounted(() => {
  observer = new ResizeObserver(([entry]) => {
    const w = Math.floor(entry.contentRect.width) - 32;
    if (Math.abs(w - width) > 8) {
      width = w;
      draw();
    }
  });
  observer.observe(container.value!);
});
onBeforeUnmount(() => observer?.disconnect());
// Après le rendu : le canvas peut venir d'apparaître.
watch(() => props.result, draw, { flush: "post" });
</script>

<template>
  <section ref="container" class="doc-preview">
    <template v-if="result">
      <header class="preview-head">
        <div class="info">
          <div class="title">
            <strong>{{ fileName(result.docPath) }}</strong>
            <span class="muted">
              {{ isImage() ? "image" : isWhole() ? `fichier entier · ${result.pageCount} p. (aperçu page 1)` : `page ${result.pageNum}` }}
            </span>
            <span class="muted">créé le {{ formatFileDate(result.createdAt) }}</span>
          </div>
          <div class="path" :title="result.docPath">{{ dirName(result.docPath) }}</div>
        </div>
        <div class="actions">
          <Button
            v-tooltip.bottom="'Glisser vers une autre application (mail, dossier…)'"
            :label="isWhole() ? 'Glisser le fichier' : 'Glisser la page'"
            icon="pi pi-arrows-alt"
            severity="secondary"
            outlined
            size="small"
            class="drag-button"
            @mouseenter="prepareDrag(result, query)"
            @mousedown="dragOnMove($event, result, query)"
          />
          <Button label="Exporter…" icon="pi pi-download" severity="secondary" outlined size="small" @click="exportCurrent" />
          <Button
            :label="`Afficher dans ${fileManager}`"
            icon="pi pi-folder-open"
            severity="secondary"
            text
            size="small"
            @click="revealItemInDir(result.docPath)"
          />
        </div>
      </header>
      <p v-if="error" class="warn">{{ error }}</p>
      <div class="canvas-wrap">
        <img v-if="isImage()" class="image-preview" :src="imgSrc" alt="" />
        <canvas v-else ref="canvas" />
      </div>
    </template>
    <p v-else class="muted empty">Sélectionnez un résultat pour afficher la page.</p>
  </section>
</template>

<style scoped>
.doc-preview {
  display: flex;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
  height: 100%;
}

.preview-head {
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--app-border);
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.title {
  display: flex;
  gap: 0.75rem;
  align-items: baseline;
  flex-wrap: wrap;
}

.path {
  font-size: 0.8em;
  color: var(--app-muted);
  user-select: text;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.actions {
  display: flex;
  gap: 0.4rem;
  flex-wrap: wrap;
}

.drag-button {
  cursor: grab;
}

.canvas-wrap {
  flex: 1;
  overflow: auto;
  padding: 16px;
  display: flex;
  justify-content: center;
  align-items: flex-start;
  background: var(--app-bg);
}

canvas,
.image-preview {
  box-shadow: 0 2px 12px rgb(0 0 0 / 0.15);
  background: #fff;
  max-width: 100%;
}

.empty {
  margin: auto;
}

.warn {
  padding: 0 1rem;
}
</style>
