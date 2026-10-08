<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import DataTable from "primevue/datatable";
import Column from "primevue/column";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import { useToast } from "primevue/usetoast";
import { api, type ClientRef, type PriceList, type PriceListItem } from "../api";
import { errorMessage, formatPct, formatUnitPrice } from "../format";

const toast = useToast();
const lists = ref<PriceList[]>([]);
const loading = ref(true);
const search = ref("");

const selected = ref<PriceList | null>(null);
const items = ref<PriceListItem[]>([]);
const listClients = ref<ClientRef[]>([]);
const itemsLoading = ref(false);
const itemSearch = ref("");

const filteredLists = computed(() => {
  const q = search.value.trim().toLowerCase();
  return q
    ? lists.value.filter((l) => l.code.toLowerCase().includes(q) || l.label?.toLowerCase().includes(q))
    : lists.value;
});

const filteredItems = computed(() => {
  const q = itemSearch.value.trim().toLowerCase();
  return q
    ? items.value.filter((i) => i.product_ref.toLowerCase().includes(q) || i.designation?.toLowerCase().includes(q))
    : items.value;
});

/** Remise de la ligne par rapport au prix public. */
const discountVsPublic = (i: PriceListItem) =>
  i.public_price ? (1 - i.price / i.public_price) * 100 : null;

async function select(list: PriceList) {
  selected.value = list;
  itemSearch.value = "";
  itemsLoading.value = true;
  try {
    [items.value, listClients.value] = await Promise.all([
      api.getPriceListItems(list.code),
      api.getPriceListClients(list.code),
    ]);
  } catch (e) {
    toast.add({ severity: "error", summary: "Chargement de la liste", detail: errorMessage(e) });
  } finally {
    itemsLoading.value = false;
  }
}

onMounted(async () => {
  try {
    lists.value = await api.listPriceLists();
  } catch (e) {
    toast.add({ severity: "error", summary: "Chargement des listes", detail: errorMessage(e) });
  } finally {
    loading.value = false;
  }
});
</script>

<template>
  <div class="page">
    <div class="page-header">
      <h1>Listes de prix</h1>
      <span class="muted">{{ lists.length }} listes</span>
    </div>
    <div class="split">
      <div class="card list-pane">
        <InputText v-model="search" placeholder="Code ou client…" fluid />
        <DataTable
          :value="filteredLists"
          :loading="loading"
          size="small"
          scrollable
          scroll-height="flex"
          selection-mode="single"
          :selection="selected"
          data-key="code"
          @row-click="select($event.data)"
        >
          <Column field="code" header="Code" sortable>
            <template #body="{ data }"><span class="mono">{{ data.code }}</span></template>
          </Column>
          <Column field="label" header="Client source" sortable />
          <Column field="item_count" header="Lignes" sortable class="num" />
          <Column field="client_count" header="Clients" sortable class="num" />
          <template #empty><span class="muted">Aucune liste.</span></template>
        </DataTable>
      </div>

      <div class="card detail-pane">
        <template v-if="selected">
          <div class="page-header">
            <h2 class="mono">{{ selected.code }}</h2>
            <span class="muted">{{ selected.label }}</span>
            <span class="spacer" />
            <InputText v-model="itemSearch" placeholder="Réf ou désignation…" size="small" />
          </div>
          <div class="clients">
            <span class="muted">Clients rattachés :</span>
            <Tag v-for="c in listClients" :key="c.code" :value="`${c.name} (${c.code})`" severity="secondary" />
            <span v-if="!listClients.length" class="muted">aucun</span>
          </div>
          <DataTable
            :value="filteredItems"
            :loading="itemsLoading"
            size="small"
            striped-rows
            scrollable
            scroll-height="flex"
            :virtual-scroller-options="{ itemSize: 36 }"
            table-class="items-table"
          >
            <!-- Largeurs fixes, sauf la désignation qui prend la place restante. -->
            <Column field="product_ref" header="Réf" sortable style="width: 120px">
              <template #body="{ data }"><span class="mono">{{ data.product_ref }}</span></template>
            </Column>
            <Column field="designation" header="Désignation" sortable>
              <template #body="{ data }">
                <span class="ellipsis" :title="data.designation ?? ''">{{ data.designation }}</span>
              </template>
            </Column>
            <Column field="family" header="Famille" style="width: 80px" />
            <Column field="price" header="Prix net" sortable class="num" style="width: 100px">
              <template #body="{ data }"><strong>{{ formatUnitPrice(data.price) }}</strong></template>
            </Column>
            <Column field="public_price" header="Prix public" class="num" style="width: 100px">
              <template #body="{ data }"><span class="muted">{{ formatUnitPrice(data.public_price) }}</span></template>
            </Column>
            <Column header="Remise" class="num" style="width: 95px">
              <template #body="{ data }">{{ formatPct(discountVsPublic(data)) }}</template>
            </Column>
            <Column field="label" header="Libellé" style="width: 150px">
              <template #body="{ data }">
                <span class="ellipsis" :title="data.label ?? ''">{{ data.label }}</span>
              </template>
            </Column>
          </DataTable>
        </template>
        <p v-else class="muted">Sélectionnez une liste pour voir ses prix.</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.page {
  height: 100%;
}

.split {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(360px, 2fr) 3fr;
  gap: 1rem;
}

.list-pane,
.detail-pane {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  min-height: 0;
}

/* Le tableau prend la hauteur restante sous la recherche / l'en-tête (et non 100 % du cadre),
   puis défile à l'intérieur : il ne déborde plus du cadre. */
.list-pane > :deep(.p-datatable),
.detail-pane > :deep(.p-datatable) {
  flex: 1;
  min-height: 0;
  height: auto;
}

/* Colonnes à largeur fixe (sauf la désignation) ; le texte trop long est coupé. */
.detail-pane :deep(.items-table) {
  table-layout: fixed;
  width: 100%;
}

.ellipsis {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.clients {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
  align-items: center;
  max-height: 80px;
  overflow: auto;
}
</style>
