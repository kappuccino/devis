<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import DataTable from "primevue/datatable";
import Column from "primevue/column";
import InputText from "primevue/inputtext";
import Button from "primevue/button";
import { useToast } from "primevue/usetoast";
import { useConfirm } from "primevue/useconfirm";
import { api, type QuoteSummary } from "../api";
import { errorMessage, formatDate, formatEuro, todayIso } from "../format";
import { exportQuotePdf } from "../composables/usePdf";
import { removeDraft } from "../drafts";

const router = useRouter();
const toast = useToast();
const confirm = useConfirm();
const quotes = ref<QuoteSummary[]>([]);
const loading = ref(true);
const search = ref("");

const filtered = computed(() => {
  const q = search.value.trim().toLowerCase();
  return q
    ? quotes.value.filter((d) =>
        [d.number, d.client_name, d.client_code ?? ""].some((v) => v.toLowerCase().includes(q)),
      )
    : quotes.value;
});

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
    const path = await exportQuotePdf(await api.getQuote(q.id));
    if (path) toast.add({ severity: "success", summary: "PDF enregistré", detail: path, life: 4000 });
  } catch (e) {
    toast.add({ severity: "error", summary: "Génération du PDF", detail: errorMessage(e) });
  }
}

onMounted(load);
</script>

<template>
  <div class="page">
    <div class="page-header">
      <h1>Devis</h1>
      <span class="muted">{{ quotes.length }}</span>
      <span class="spacer" />
      <InputText v-model="search" placeholder="N°, client…" style="width: 240px" />
      <Button label="Nouveau devis" icon="pi pi-plus" @click="router.push('/devis/nouveau')" />
    </div>
    <DataTable
      :value="filtered"
      :loading="loading"
      paginator
      :rows="50"
      size="small"
      striped-rows
      class="card"
      @row-dblclick="router.push(`/devis/${$event.data.id}`)"
    >
      <Column field="number" header="N°" sortable>
        <template #body="{ data }">
          <RouterLink :to="`/devis/${data.id}`" class="mono">{{ data.number }}</RouterLink>
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
      <Column header="" style="width: 150px">
        <template #body="{ data }">
          <div class="row-actions">
            <Button v-tooltip.top="'PDF'" icon="pi pi-file-pdf" text rounded size="small" @click="pdf(data)" />
            <Button v-tooltip.top="'Dupliquer'" icon="pi pi-copy" text rounded size="small" @click="duplicate(data)" />
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
.strike {
  margin-left: 0.4rem;
  text-decoration: line-through;
  font-size: 0.9em;
}

.row-actions {
  display: flex;
  justify-content: flex-end;
}
</style>
