<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import DataTable from "primevue/datatable";
import Column from "primevue/column";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import { useToast } from "primevue/usetoast";
import { api, type Client } from "../api";
import { errorMessage, formatPct } from "../format";
import ClientFormDialog from "../components/ClientFormDialog.vue";

const toast = useToast();
const clients = ref<Client[]>([]);
const loading = ref(true);
const search = ref("");

const dialogVisible = ref(false);
const editing = ref<Client | null>(null);

const filtered = computed(() => {
  const q = search.value.trim().toLowerCase();
  if (!q) return clients.value;
  return clients.value.filter((c) =>
    [c.code, c.name, c.group_name, c.subgroup, c.sales_rep, c.siren].some((v) => v?.toLowerCase().includes(q)),
  );
});

async function load() {
  loading.value = true;
  try {
    clients.value = await api.listClients();
  } catch (e) {
    toast.add({ severity: "error", summary: "Chargement des clients", detail: errorMessage(e) });
  } finally {
    loading.value = false;
  }
}

function openClient(client: Client) {
  editing.value = client;
  dialogVisible.value = true;
}

function onSaved(saved: Client) {
  const i = clients.value.findIndex((c) => c.code === saved.code);
  if (i >= 0) clients.value[i] = saved;
}

onMounted(load);
</script>

<template>
  <div class="page">
    <div class="page-header">
      <h1>Clients</h1>
      <span class="muted">{{ filtered.length }} / {{ clients.length }}</span>
      <span class="spacer" />
      <InputText v-model="search" placeholder="Nom, code, groupe, commercial…" style="width: 280px" />
    </div>
    <DataTable
      :value="filtered"
      :loading="loading"
      paginator
      :rows="500"
      :rows-per-page-options="[50, 100, 250, 500]"
      size="small"
      striped-rows
      selection-mode="single"
      data-key="code"
      class="card"
      @row-click="openClient($event.data)"
    >
      <Column field="code" header="Code" sortable>
        <template #body="{ data }">
          <span class="mono">{{ data.code }}</span>
        </template>
      </Column>
      <Column field="name" header="Raison sociale" sortable />
      <Column field="group_name" header="Groupe" sortable />
      <Column field="sales_rep" header="Commercial" sortable />
      <Column field="discount_cfa" header="Remise CFA" sortable class="num">
        <template #body="{ data }">{{ formatPct(data.discount_cfa) }}</template>
      </Column>
      <Column field="discount_cfo" header="Remise CFO" sortable class="num">
        <template #body="{ data }">{{ formatPct(data.discount_cfo) }}</template>
      </Column>
      <Column header="Listes de prix">
        <template #body="{ data }">
          <div class="tags">
            <Tag v-for="l in data.price_lists" :key="l" :value="l" severity="secondary" />
          </div>
        </template>
      </Column>
      <template #empty>
        <span class="muted">Aucun client. Importez le fichier LPN depuis les Réglages.</span>
      </template>
    </DataTable>

    <ClientFormDialog v-model:visible="dialogVisible" :client="editing" @saved="onSaved" />
  </div>
</template>

<style scoped>
.tags {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}
</style>
