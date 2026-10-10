<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import DataTable from "primevue/datatable";
import Column from "primevue/column";
import InputText from "primevue/inputtext";
import SelectButton from "primevue/selectbutton";
import Tag from "primevue/tag";
import { useToast } from "primevue/usetoast";
import { api, type Product } from "../api";
import { errorMessage, formatUnitPrice } from "../format";

const toast = useToast();
const products = ref<Product[]>([]);
const loading = ref(true);
const search = ref("");
const family = ref<"Tous" | "CFA" | "CFO">("Tous");

const filtered = computed(() => {
  const q = search.value.trim().toLowerCase();
  return products.value.filter(
    (p) =>
      (family.value === "Tous" || p.family === family.value) &&
      (!q || p.ref.toLowerCase().includes(q) || p.designation.toLowerCase().includes(q) || p.enedis_code?.includes(q)),
  );
});

onMounted(async () => {
  try {
    products.value = await api.listProducts();
  } catch (e) {
    toast.add({ severity: "error", summary: "Chargement des produits", detail: errorMessage(e) });
  } finally {
    loading.value = false;
  }
});
</script>

<template>
  <div class="page">
    <div class="page-header">
      <h1>Produits</h1>
      <span class="muted">{{ filtered.length }} / {{ products.length }}</span>
      <span class="spacer" />
      <SelectButton v-model="family" :options="['Tous', 'CFA', 'CFO']" :allow-empty="false" />
      <InputText v-model="search" placeholder="Réf, désignation, code ENEDIS…" style="width: 280px" />
    </div>
    <DataTable
      :value="filtered"
      :loading="loading"
      paginator
      :rows="500"
      :rows-per-page-options="[50, 100, 250, 500]"
      size="small"
      striped-rows
      sort-mode="single"
      class="card"
    >
      <Column field="ref" header="Réf" sortable>
        <template #body="{ data }"><span class="mono">{{ data.ref }}</span></template>
      </Column>
      <Column field="designation" header="Désignation" sortable />
      <Column field="enedis_code" header="Code ENEDIS" />
      <Column field="family" header="Famille" sortable>
        <template #body="{ data }">
          <Tag v-if="data.family" :value="data.family" :severity="data.family === 'CFO' ? 'warn' : 'info'" />
        </template>
      </Column>
      <Column field="public_price" header="Prix public" sortable class="num">
        <template #body="{ data }">{{ formatUnitPrice(data.public_price) }}</template>
      </Column>
      <Column field="threshold_price" header="Prix seuil" sortable class="num">
        <template #body="{ data }"><span class="muted">{{ formatUnitPrice(data.threshold_price) }}</span></template>
      </Column>
      <Column field="eco_tax" header="Éco-taxe" class="num">
        <template #body="{ data }">{{ formatUnitPrice(data.eco_tax) }}</template>
      </Column>
      <template #empty>
        <span class="muted">Aucun produit. Importez le fichier LPN depuis les Réglages.</span>
      </template>
    </DataTable>
  </div>
</template>
