<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useRouter } from "vue-router";
import Dialog from "primevue/dialog";
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";
import Message from "primevue/message";
import ProgressSpinner from "primevue/progressspinner";
import { useToast } from "primevue/usetoast";
import { api, type Quote } from "../../api";
import { askQuotePdfPath, quotePdfBytes, writeAndOpenPdf } from "../../composables/usePdf";
import {
  assembleQuoteWithDocs,
  buildProposals,
  externalDoc,
  includedParts,
  toAttachments,
  type ExternalDoc,
  type ProductGroup,
  type Proposal,
} from "../../docs/attachments";
import { fileKind } from "../../docs/core/index.js";
import { dirName, fileName } from "../../docs/files";
import type { SearchResult } from "../../docs/search";
import { docs, indexing } from "../../docs/store";
import { errorMessage } from "../../format";
import DocPagePreview from "./DocPagePreview.vue";
import { rasterizer } from "../../docs/pages";

/** Devis enregistré (avec son id) dont on génère le PDF + la documentation. */
const props = defineProps<{ quote: Quote | null }>();
const visible = defineModel<boolean>("visible", { required: true });

const router = useRouter();
const toast = useToast();
const loading = ref(false);
const generating = ref(false);
const groups = ref<ProductGroup[]>([]);
const externals = ref<ExternalDoc[]>([]);
/** Groupes dont les correspondances approximatives sont dépliées. */
const openApprox = ref(new Set<string>());
/** Élément affiché dans l'aperçu. */
const focused = ref<{ result: SearchResult; ref: string } | null>(null);

watch(visible, async (isOpen) => {
  if (!isOpen || !props.quote?.id) return;
  loading.value = true;
  groups.value = [];
  externals.value = [];
  focused.value = null;
  openApprox.value = new Set();
  try {
    const saved = await api.getQuoteAttachments(props.quote.id);
    const found = await buildProposals(props.quote.lines, saved);
    groups.value = found.groups;
    externals.value = found.externals;
    const first = found.groups.find((g) => g.proposals.length);
    if (first) focused.value = { result: first.proposals[0].result, ref: first.ref };
  } catch (e) {
    toast.add({ severity: "error", summary: "Recherche de la documentation", detail: errorMessage(e) });
  } finally {
    loading.value = false;
  }
});

const where = (r: SearchResult) =>
  r.kind === "image" ? "image" : r.pageNum == null ? `fichier entier${r.pageCount ? ` · ${r.pageCount} p.` : ""}` : `page ${r.pageNum}`;

const isFocused = (r: SearchResult) => focused.value?.result.docPath === r.docPath && focused.value?.result.pageNum === r.pageNum;

function focus(p: Proposal, group: ProductGroup) {
  focused.value = { result: p.result, ref: group.ref };
}

/** Aperçu d'un document ajouté (fichier entier). */
function focusExternal(e: ExternalDoc) {
  if (e.missing) return;
  focused.value = {
    ref: "",
    result: {
      docPath: e.path,
      kind: fileKind(e.path) === "image" ? "image" : "pdf",
      pageNum: null,
      pageCount: 0,
      createdAt: null,
      nameMatch: false,
      nameHighlight: null,
      match: "filename",
      snippet: "",
      highlight: null,
    },
  };
}

function toggleApprox(ref: string) {
  const s = new Set(openApprox.value);
  if (s.has(ref)) s.delete(ref);
  else s.add(ref);
  openApprox.value = s;
}

async function addExternal() {
  const picked = await open({
    multiple: true,
    directory: false,
    title: "Documents à joindre au devis",
    filters: [{ name: "PDF ou image", extensions: ["pdf", "jpg", "jpeg", "png"] }],
  });
  const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
  for (const path of paths) {
    if (!externals.value.some((e) => e.path === path)) externals.value.push(externalDoc(path));
  }
  if (paths.length) focusExternal(externals.value[externals.value.length - 1]);
}

function removeExternal(e: ExternalDoc) {
  externals.value = externals.value.filter((x) => x !== e);
  if (focused.value?.result.docPath === e.path) focused.value = null;
}

const chosenCount = computed(
  () =>
    groups.value.reduce((n, g) => n + [...g.proposals, ...g.approx].filter((p) => p.included).length, 0) +
    externals.value.filter((e) => e.included && !e.missing).length,
);

async function generate() {
  const quote = props.quote;
  if (!quote?.id) return;
  generating.value = true;
  try {
    // Les choix sont mémorisés avec le devis, même si l'enregistrement du PDF est annulé.
    await api.saveQuoteAttachments(quote.id, toAttachments(groups.value, externals.value));
    const path = await askQuotePdfPath(quote, " + docs");
    if (!path) return;
    const { bytes, warnings } = await assembleQuoteWithDocs(
      await quotePdfBytes(quote),
      includedParts(groups.value, externals.value),
      rasterizer,
    );
    await writeAndOpenPdf(path, bytes);
    toast.add({ severity: "success", summary: "PDF enregistré", detail: path, life: 4000 });
    if (warnings.length) {
      toast.add({ severity: "warn", summary: "Documents ignorés (illisibles)", detail: warnings.join("\n") });
    }
    visible.value = false;
  } catch (e) {
    toast.add({ severity: "error", summary: "Génération du PDF", detail: errorMessage(e) });
  } finally {
    generating.value = false;
  }
}
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    header="Devis PDF + documentation"
    :style="{ width: '92vw', maxWidth: '1400px', height: '88vh' }"
    :content-style="{ flex: 1, minHeight: 0, padding: 0, overflow: 'hidden', display: 'flex' }"
    :pt="{ footer: { style: 'border-top: 1px solid var(--app-border); padding: 1rem 1.5rem; gap: 0.5rem' } }"
    :draggable="false"
  >
    <div class="docs-dialog">
      <!-- Propositions par produit -->
      <div class="choices">
        <Message v-if="!docs.folder" severity="warn" :closable="false" class="note">
          Aucun dossier de documentation n'est choisi.
          <Button label="Réglages" text size="small" @click="router.push('/reglages/documentation')" />
        </Message>
        <Message v-else-if="indexing" severity="info" :closable="false" class="note">
          Indexation de la documentation en cours : les propositions peuvent être incomplètes.
        </Message>

        <div v-if="loading" class="loading">
          <ProgressSpinner style="width: 36px; height: 36px" />
          <span class="muted">Recherche de la documentation des produits…</span>
        </div>

        <template v-else>
          <section v-for="g in groups" :key="g.ref" class="group">
            <header>
              <span class="mono ref">{{ g.ref }}</span>
              <span class="designation">{{ g.designation }}</span>
            </header>
            <p v-if="!g.proposals.length && !g.approx.length" class="muted none">Aucun document trouvé.</p>
            <div
              v-for="p in g.proposals"
              :key="p.key"
              class="proposal"
              :class="{ focused: isFocused(p.result), off: !p.included }"
              @click="focus(p, g)"
            >
              <Checkbox v-model="p.included" binary @click.stop />
              <div class="text">
                <div class="line">
                  <span class="name">{{ fileName(p.result.docPath) }}</span>
                  <span class="muted where">{{ where(p.result) }}</span>
                  <span v-if="p.result.nameMatch" class="badge">nom du fichier</span>
                  <span v-if="p.result.match === 'prefix'" class="badge">début de réf.</span>
                </div>
                <div v-if="p.result.snippet" class="snippet">{{ p.result.snippet }}</div>
              </div>
            </div>
            <template v-if="g.approx.length">
              <Button
                :label="`${openApprox.has(g.ref) ? 'Masquer' : 'Voir'} ${g.approx.length} correspondance(s) approximative(s)`"
                :icon="openApprox.has(g.ref) ? 'pi pi-chevron-up' : 'pi pi-chevron-down'"
                text
                size="small"
                severity="secondary"
                @click="toggleApprox(g.ref)"
              />
              <template v-if="openApprox.has(g.ref)">
                <div
                  v-for="p in g.approx"
                  :key="p.key"
                  class="proposal approx"
                  :class="{ focused: isFocused(p.result), off: !p.included }"
                  @click="focus(p, g)"
                >
                  <Checkbox v-model="p.included" binary @click.stop />
                  <div class="text">
                    <div class="line">
                      <span class="name">{{ fileName(p.result.docPath) }}</span>
                      <span class="muted where">{{ where(p.result) }}</span>
                      <span class="badge warn">approximatif</span>
                    </div>
                    <div v-if="p.result.snippet" class="snippet">{{ p.result.snippet }}</div>
                  </div>
                </div>
              </template>
            </template>
          </section>
          <p v-if="!groups.length" class="muted none">Aucun produit dans ce devis.</p>

          <!-- Documents ajoutés à la main -->
          <section class="group">
            <header>
              <span class="ref">Documents ajoutés</span>
              <Button label="Ajouter un document…" icon="pi pi-plus" text size="small" @click="addExternal" />
            </header>
            <p v-if="!externals.length" class="muted none">PDF ou image pris sur l'ordinateur, joint en entier à la fin.</p>
            <div
              v-for="e in externals"
              :key="e.key"
              class="proposal"
              :class="{ focused: focused?.result.docPath === e.path, off: !e.included || e.missing }"
              @click="focusExternal(e)"
            >
              <Checkbox v-model="e.included" binary :disabled="e.missing" @click.stop />
              <div class="text">
                <div class="line">
                  <span class="name">{{ fileName(e.path) }}</span>
                  <span v-if="e.missing" class="badge warn">introuvable : ignoré</span>
                </div>
                <div class="snippet muted path">{{ dirName(e.path) }}</div>
              </div>
              <Button
                v-tooltip.left="'Retirer'"
                icon="pi pi-times"
                text
                rounded
                size="small"
                severity="secondary"
                @click.stop="removeExternal(e)"
              />
            </div>
          </section>
        </template>
      </div>

      <!-- Aperçu -->
      <DocPagePreview :result="focused?.result ?? null" :query="focused?.ref ?? ''" class="preview" />
    </div>

    <template #footer>
      <span class="muted summary">
        {{ chosenCount }} document(s) coché(s) · le PDF enchaîne le devis, une page de transition, puis la documentation
      </span>
      <Button label="Annuler" severity="secondary" text @click="visible = false" />
      <Button label="Générer le PDF" icon="pi pi-file-pdf" :loading="generating" :disabled="loading" @click="generate" />
    </template>
  </Dialog>
</template>

<style scoped>
/* La grille occupe la place entre l'en-tête et le pied de la fenêtre ;
   chaque colonne défile à l'intérieur (l'aperçu ne passe plus sous les boutons). */
.docs-dialog {
  flex: 1;
  display: grid;
  grid-template-columns: minmax(360px, 44%) 1fr;
  grid-template-rows: minmax(0, 1fr);
  min-height: 0;
  border-top: 1px solid var(--app-border);
}

.choices {
  overflow: auto;
  border-right: 1px solid var(--app-border);
  padding: 0.5rem 0;
}

.note {
  margin: 0.5rem 0.75rem;
}

.loading {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.75rem;
  padding: 3rem 1rem;
}

.group {
  padding: 0.5rem 0;
  border-bottom: 1px solid var(--app-border);
}

.group header {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding: 0.25rem 0.9rem 0.4rem;
}

.ref {
  font-weight: 700;
}

.designation {
  color: var(--app-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.none {
  margin: 0;
  padding: 0.25rem 0.9rem 0.5rem;
  font-size: 0.9em;
}

.proposal {
  display: flex;
  align-items: flex-start;
  gap: 0.6rem;
  padding: 0.45rem 0.9rem;
  cursor: pointer;
}

.proposal:hover {
  background: var(--app-bg);
}

.proposal.focused {
  background: color-mix(in srgb, var(--app-accent) 10%, transparent);
  box-shadow: inset 3px 0 0 var(--app-accent);
}

.proposal.off .text {
  opacity: 0.55;
}

.text {
  flex: 1;
  min-width: 0;
}

.line {
  display: flex;
  gap: 0.5rem;
  align-items: baseline;
  flex-wrap: wrap;
}

.name {
  font-weight: 600;
}

.where {
  font-size: 0.85em;
}

.snippet {
  font-size: 0.85em;
  color: var(--app-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.badge {
  font-size: 0.75em;
  padding: 0 0.4rem;
  border-radius: 999px;
  border: 1px solid var(--app-border);
  color: var(--app-muted);
}

.badge.warn {
  color: var(--app-warn);
  border-color: color-mix(in srgb, var(--app-warn) 40%, transparent);
}

.preview {
  min-width: 0;
}

.summary {
  margin-right: auto;
  font-size: 0.9em;
}
</style>
