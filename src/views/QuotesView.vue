<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import DataTable from "primevue/datatable";
import Column from "primevue/column";
import InputText from "primevue/inputtext";
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";
import Select from "primevue/select";
import { useToast } from "primevue/usetoast";
import { useConfirm } from "primevue/useconfirm";
import { api, type QuoteSummary, type QuoteTemplate } from "../api";
import { errorMessage, formatDate, formatEuro, todayIso } from "../format";
import { exportQuotePdf } from "../composables/usePdf";
import { removeDraft } from "../drafts";

const router = useRouter();
const route = useRoute();
const toast = useToast();

/** Onglet : devis, ou devis types (modèles sans client ni prix) ; gardé dans l'URL (?onglet=types). */
const tab = computed<"quotes" | "types">(() => (route.query.onglet === "types" ? "types" : "quotes"));
const setTab = (t: "quotes" | "types") => router.replace({ query: t === "types" ? { onglet: "types" } : {} });

const templates = ref<QuoteTemplate[]>([]);
const templateSearch = ref("");
const filteredTemplates = computed(() => {
  const q = templateSearch.value.trim().toLowerCase();
  return q ? templates.value.filter((t) => t.name.toLowerCase().includes(q)) : templates.value;
});

async function loadTemplates() {
  try {
    templates.value = await api.listQuoteTemplates();
  } catch (e) {
    toast.add({ severity: "error", summary: "Chargement des devis types", detail: errorMessage(e) });
  }
}

function removeTemplate(t: QuoteTemplate) {
  confirm.require({
    message: `Supprimer le devis type « ${t.name} » ? Les devis déjà créés à partir de lui ne changent pas.`,
    header: "Suppression",
    icon: "pi pi-trash",
    rejectProps: { label: "Annuler", severity: "secondary", text: true },
    acceptProps: { label: "Supprimer", severity: "danger" },
    accept: async () => {
      try {
        await api.deleteQuote(t.id);
        removeDraft(String(t.id));
        await loadTemplates();
      } catch (e) {
        toast.add({ severity: "error", summary: "Suppression", detail: errorMessage(e) });
      }
    },
  });
}
const confirm = useConfirm();
const quotes = ref<QuoteSummary[]>([]);
const loading = ref(true);
const search = ref("");
/** Suivi : tous les devis, affaires obtenues ou en attente. */
const status = ref<"all" | "won" | "open">("all");
const statusOptions = [
  { label: "Tous les devis", value: "all" },
  { label: "Affaires obtenues", value: "won" },
  { label: "En attente", value: "open" },
];

const filtered = computed(() => {
  const q = search.value.trim().toLowerCase();
  return quotes.value.filter(
    (d) =>
      (status.value === "all" || (status.value === "won") === !!d.won_at) &&
      (!q || [d.number, d.client_name, d.client_code ?? "", d.project_name].some((v) => v.toLowerCase().includes(q))),
  );
});

async function setWon(q: QuoteSummary, won: boolean) {
  try {
    await api.setQuoteWon(q.id, won);
    // Une seule version d'un devis peut être obtenue : les autres versions sont décochées.
    quotes.value = await api.listQuotes();
  } catch (e) {
    toast.add({ severity: "error", summary: "Affaire obtenue", detail: errorMessage(e) });
  }
}

async function load() {
  loading.value = true;
  try {
    quotes.value = await api.listQuotes();
  } catch (e) {
    toast.add({ severity: "error", summary: "Chargement des devis", detail: errorMessage(e) });
  } finally {
    loading.value = false;
  }
}

async function duplicate(q: QuoteSummary) {
  try {
    const copy = await api.duplicateQuote(q.id, todayIso());
    router.push(`/devis/${copy.id}`);
  } catch (e) {
    toast.add({ severity: "error", summary: "Duplication", detail: errorMessage(e) });
  }
}

async function newVersion(q: QuoteSummary) {
  try {
    const v = await api.newQuoteVersion(q.id, todayIso());
    toast.add({ severity: "success", summary: `Version ${v.number} créée`, life: 2500 });
    router.push(`/devis/${v.id}`);
  } catch (e) {
    toast.add({ severity: "error", summary: "Nouvelle version", detail: errorMessage(e) });
  }
}

function remove(q: QuoteSummary) {
  confirm.require({
    message: `Supprimer le devis ${q.number} (${q.client_name}) ?`,
    header: "Suppression",
    icon: "pi pi-trash",
    rejectProps: { label: "Annuler", severity: "secondary", text: true },
    acceptProps: { label: "Supprimer", severity: "danger" },
    accept: async () => {
      try {
        await api.deleteQuote(q.id);
        removeDraft(String(q.id));
        await load();
      } catch (e) {
        toast.add({ severity: "error", summary: "Suppression", detail: errorMessage(e) });
      }
    },
  });
}

async function pdf(q: QuoteSummary) {
  try {
    const result = await exportQuotePdf(await api.getQuote(q.id));
    if (result) {
      toast.add({ severity: "success", summary: "PDF enregistré", detail: result.path, life: 4000 });
      if (result.warning) toast.add({ severity: "warn", summary: "Conditions générales de vente", detail: result.warning });
    }
  } catch (e) {
    toast.add({ severity: "error", summary: "Génération du PDF", detail: errorMessage(e) });
  }
}

onMounted(() => {
  load();
  loadTemplates();
});
</script>

<template>
  <div class="page">
    <div class="page-header">
      <h1>Devis</h1>
      <nav class="tabs" aria-label="Devis ou devis types">
        <button type="button" class="tab" :class="{ active: tab === 'quotes' }" @click="setTab('quotes')">
          Devis <span class="count">{{ quotes.length }}</span>
        </button>
        <button type="button" class="tab" :class="{ active: tab === 'types' }" @click="setTab('types')">
          Devis types <span class="count">{{ templates.length }}</span>
        </button>
      </nav>
      <span class="spacer" />
      <template v-if="tab === 'quotes'">
        <Select v-model="status" :options="statusOptions" option-label="label" option-value="value" aria-label="Suivi" />
        <InputText v-model="search" placeholder="N°, client, affaire…" style="width: 240px" />
        <Button label="Nouveau devis" icon="pi pi-plus" @click="router.push('/devis/nouveau')" />
      </template>
      <template v-else>
        <InputText v-model="templateSearch" placeholder="Nom…" style="width: 240px" />
        <Button label="Nouveau devis type" icon="pi pi-plus" @click="router.push('/devis-types/nouveau')" />
      </template>
    </div>

    <!-- Devis types : modèles sans client ni prix, pour démarrer un devis. -->
    <DataTable
      v-if="tab === 'types'"
      :value="filteredTemplates"
      size="small"
      striped-rows
      class="card"
      @row-dblclick="router.push(`/devis-types/${$event.data.id}`)"
    >
      <Column field="name" header="Nom" sortable>
        <template #body="{ data }">
          <RouterLink :to="`/devis-types/${data.id}`">{{ data.name }}</RouterLink>
          <span v-if="data.notes" class="muted note">{{ data.notes }}</span>
        </template>
      </Column>
      <Column field="line_count" header="Articles" class="num" sortable style="width: 100px" />
      <Column field="updated_at" header="Modifié le" sortable style="width: 140px">
        <template #body="{ data }">{{ formatDate(data.updated_at.slice(0, 10)) }}</template>
      </Column>
      <Column header="" style="width: 230px">
        <template #body="{ data }">
          <div class="row-actions">
            <Button
              v-tooltip.top="'Nouveau devis à partir de ce type (prix calculés pour le client choisi)'"
              label="Créer un devis"
              icon="pi pi-file-plus"
              text
              size="small"
              @click="router.push({ path: '/devis/nouveau', query: { type: data.id } })"
            />
            <Button
              v-tooltip.top="'Supprimer'"
              icon="pi pi-trash"
              text
              rounded
              size="small"
              severity="danger"
              @click="removeTemplate(data)"
            />
          </div>
        </template>
      </Column>
      <template #empty>
        <span class="muted">
          Aucun devis type. Créez-en un ici, ou depuis un devis : bouton <i class="pi pi-bookmark" /> « Enregistrer
          comme devis type ».
        </span>
      </template>
    </DataTable>

    <DataTable
      v-else
      :value="filtered"
      :loading="loading"
      paginator
      :rows="50"
      size="small"
      striped-rows
      class="card"
      :row-class="(d: QuoteSummary) => (d.superseded ? 'superseded' : '')"
      @row-dblclick="router.push(`/devis/${$event.data.id}`)"
    >
      <Column field="number" header="N°" sortable>
        <template #body="{ data }">
          <RouterLink :to="`/devis/${data.id}`" class="mono">{{ data.number }}</RouterLink>
          <span v-if="data.superseded" v-tooltip.top="'Une version plus récente de ce devis existe'" class="muted old">
            ancienne version
          </span>
        </template>
      </Column>
      <Column field="date" header="Date" sortable>
        <template #body="{ data }">{{ formatDate(data.date) }}</template>
      </Column>
      <Column field="client_name" header="Client" sortable>
        <template #body="{ data }">
          {{ data.client_name }}
          <span v-if="data.client_code" class="muted mono">{{ data.client_code }}</span>
          <span v-else class="muted">(ponctuel)</span>
          <div v-if="data.project_name" class="muted project">{{ data.project_name }}</div>
        </template>
      </Column>
      <Column field="line_count" header="Lignes" class="num" />
      <Column field="total_net" header="Total HT" sortable class="num">
        <template #body="{ data }">
          <strong>{{ formatEuro(data.total_net) }}</strong>
          <span v-if="data.total_net !== data.total_ht" v-tooltip.top="'Avant remise globale'" class="muted strike">
            {{ formatEuro(data.total_ht) }}
          </span>
        </template>
      </Column>
      <Column field="won_at" header="Obtenue" sortable style="width: 90px">
        <template #body="{ data }">
          <Checkbox
            v-tooltip.top="data.won_at ? `Affaire obtenue (cochée le ${formatDate(data.won_at)})` : 'Marquer l\'affaire obtenue'"
            :model-value="!!data.won_at"
            binary
            :aria-label="`Affaire obtenue : ${data.number}`"
            @update:model-value="setWon(data, $event)"
          />
        </template>
      </Column>
      <Column header="" style="width: 180px">
        <template #body="{ data }">
          <div class="row-actions">
            <Button v-tooltip.top="'PDF'" icon="pi pi-file-pdf" text rounded size="small" @click="pdf(data)" />
            <Button
              v-tooltip.top="'Nouvelle version (même numéro, suffixe -V2, -V3…)'"
              icon="pi pi-history"
              text
              rounded
              size="small"
              @click="newVersion(data)"
            />
            <Button
              v-tooltip.top="'Dupliquer (nouveau devis, nouveau numéro)'"
              icon="pi pi-copy"
              text
              rounded
              size="small"
              @click="duplicate(data)"
            />
            <Button
              v-tooltip.top="'Supprimer'"
              icon="pi pi-trash"
              text
              rounded
              size="small"
              severity="danger"
              @click="remove(data)"
            />
          </div>
        </template>
      </Column>
      <template #empty><span class="muted">Aucun devis pour l'instant.</span></template>
    </DataTable>
  </div>
</template>

<style scoped>
.tabs {
  display: flex;
  gap: 2px;
  padding: 3px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--app-muted) 12%, transparent);
}

.tab {
  padding: 4px 12px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--app-muted);
  font: inherit;
  font-weight: 600;
  cursor: pointer;
}

.tab.active {
  background: var(--app-surface);
  color: var(--app-text);
  box-shadow: 0 1px 2px rgb(0 0 0 / 0.08);
}

.count {
  margin-left: 0.25rem;
  font-weight: 400;
  color: var(--app-muted);
}

.project {
  font-size: 0.85em;
}

.note {
  margin-left: 0.6rem;
  font-size: 0.85em;
}

.strike {
  margin-left: 0.4rem;
  text-decoration: line-through;
  font-size: 0.9em;
}

.old {
  margin-left: 0.4rem;
  font-size: 0.8em;
}

/* Ancienne version : estompée (une version plus récente existe). */
:deep(tr.superseded) td {
  color: var(--app-muted);
}

:deep(tr.superseded) a {
  color: inherit;
}

.row-actions {
  display: flex;
  justify-content: flex-end;
}
</style>
