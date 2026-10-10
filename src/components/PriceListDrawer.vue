<script setup lang="ts">
// Tiroir latéral : tous les produits d'une liste de prix, avec une recherche.
// Ouvert depuis le devis (listes du client, liste forcée).
import { computed, ref, watch } from "vue";
import Drawer from "primevue/drawer";
import DataTable from "primevue/datatable";
import Column from "primevue/column";
import InputText from "primevue/inputtext";
import IconField from "primevue/iconfield";
import InputIcon from "primevue/inputicon";
import { useToast } from "primevue/usetoast";
import { api, type PriceListItem } from "../api";
import { errorMessage, formatPct, formatUnitPrice } from "../format";

/** Code de la liste affichée ; null : tiroir fermé. */
const code = defineModel<string | null>("code", { required: true });

const toast = useToast();
const items = ref<PriceListItem[]>([]);
const label = ref<string | null>(null);
const loading = ref(false);
const search = ref("");

const visible = computed({
  get: () => code.value != null,
  set: (v) => {
    if (!v) code.value = null;
  },
});

const filtered = computed(() => {
  const q = search.value.trim().toLowerCase();
  return q
    ? items.value.filter((i) => i.product_ref.toLowerCase().includes(q) || i.designation?.toLowerCase().includes(q))
    : items.value;
});

/** Remise de la ligne par rapport au prix public. */
const discountVsPublic = (i: PriceListItem) => (i.public_price ? (1 - i.price / i.public_price) * 100 : null);

watch(code, async (c) => {
  if (!c) return;
  search.value = "";
  items.value = [];
  label.value = null;
  loading.value = true;
  try {
    const [rows, lists] = await Promise.all([api.getPriceListItems(c), api.listPriceLists()]);
    if (code.value !== c) return;
    items.value = rows;
    label.value = lists.find((l) => l.code === c)?.label ?? null;
  } catch (e) {
    toast.add({ severity: "error", summary: `Liste ${c}`, detail: errorMessage(e) });
  } finally {
    loading.value = false;
  }
});
</script>

<template>
  <Drawer v-model:visible="visible" position="right" class="price-list-drawer" :style="{ width: 'min(760px, 92vw)' }">
    <template #header>
      <div class="drawer-title">
        <h2 class="mono">{{ code }}</h2>
        <span class="muted">{{ label }}<template v-if="!loading"> · {{ items.length }} produits</template></span>
      </div>
    </template>
    <IconField>
      <InputIcon class="pi pi-search" />
      <InputText v-model="search" placeholder="Réf ou désignation…" fluid autofocus />
    </IconField>
    <DataTable
      :value="filtered"
      :loading="loading"
      size="small"
      striped-rows
      scrollable
      scroll-height="flex"
      :virtual-scroller-options="{ itemSize: 36 }"
      table-class="items-table"
      class="items"
    >
      <Column field="product_ref" header="Réf" sortable style="width: 110px">
        <template #body="{ data }"><span class="mono">{{ data.product_ref }}</span></template>
      </Column>
      <Column field="designation" header="Désignation" sortable>
        <template #body="{ data }">
          <span class="ellipsis" :title="data.designation ?? ''">{{ data.designation }}</span>
        </template>
      </Column>
      <Column field="price" header="Prix net" sortable class="num" style="width: 95px">
        <template #body="{ data }"><strong>{{ formatUnitPrice(data.price) }}</strong></template>
      </Column>
      <Column field="public_price" header="Prix public" class="num" style="width: 95px">
        <template #body="{ data }"><span class="muted">{{ formatUnitPrice(data.public_price) }}</span></template>
      </Column>
      <Column header="Remise" class="num" style="width: 80px">
        <template #body="{ data }">{{ formatPct(discountVsPublic(data)) }}</template>
      </Column>
      <template #empty>
        <span class="muted">{{ search ? "Aucun produit ne correspond." : "Aucun produit dans cette liste." }}</span>
      </template>
    </DataTable>
  </Drawer>
</template>

<style scoped>
.drawer-title {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
}

.drawer-title h2 {
  margin: 0;
}

/* Le tableau prend la hauteur restante sous la recherche, puis défile. */
.items {
  flex: 1;
  min-height: 0;
  margin-top: 0.75rem;
}

.items :deep(.items-table) {
  table-layout: fixed;
  width: 100%;
}

.ellipsis {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>

<style>
/* Contenu en colonne pour que le tableau défile (largeur : style du tiroir, plus fort que le thème). */
.p-drawer.price-list-drawer .p-drawer-content {
  display: flex;
  flex-direction: column;
}
</style>
