<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Dialog from "primevue/dialog";
import InputNumber from "primevue/inputnumber";
import MultiSelect from "primevue/multiselect";
import Button from "primevue/button";
import { useToast } from "primevue/usetoast";
import { api, type Client, type PriceList } from "../api";
import { errorMessage } from "../format";

/** Modification des remises et listes de prix d'un client de la base (le reste vient du fichier Excel). */
const props = defineProps<{ client: Client | null }>();
const visible = defineModel<boolean>("visible", { required: true });
const emit = defineEmits<{ saved: [client: Client] }>();

const toast = useToast();
const priceLists = ref<PriceList[]>([]);
const saving = ref(false);
const form = ref({ discount_cfa: 0, discount_cfo: 0, price_lists: [] as string[] });

const listOptions = computed(() =>
  priceLists.value.map((l) => ({ code: l.code, label: `${l.code}${l.label ? ` — ${l.label}` : ""} (${l.item_count})` })),
);

watch(visible, async (open) => {
  const c = props.client;
  if (!open || !c) return;
  form.value = { discount_cfa: c.discount_cfa, discount_cfo: c.discount_cfo, price_lists: [...c.price_lists] };
  if (!priceLists.value.length) {
    try {
      priceLists.value = await api.listPriceLists();
    } catch (e) {
      toast.add({ severity: "error", summary: "Listes de prix", detail: errorMessage(e) });
    }
  }
});

async function save() {
  const c = props.client;
  if (!c) return;
  saving.value = true;
  try {
    const { discount_cfa, discount_cfo, price_lists } = form.value;
    await api.updateClient(c.code, discount_cfa, discount_cfo, price_lists);
    toast.add({ severity: "success", summary: "Client enregistré", life: 2000 });
    emit("saved", { ...c, discount_cfa, discount_cfo, price_lists: [...price_lists] });
    visible.value = false;
  } catch (e) {
    toast.add({ severity: "error", summary: "Enregistrement", detail: errorMessage(e) });
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <Dialog v-model:visible="visible" modal :header="client?.name" :style="{ width: '560px' }" :draggable="false">
    <div v-if="client" class="form-grid">
      <label>Code</label><span class="mono">{{ client.code }}</span>
      <label>Groupe</label><span>{{ client.group_name }} {{ client.subgroup ? `/ ${client.subgroup}` : "" }}</span>
      <label>Commercial</label><span>{{ client.sales_rep }}</span>
      <label>Email</label><span>{{ client.email }}</span>
      <label>SIREN</label><span>{{ client.siren }}</span>
      <label>Franco</label><span>{{ client.franco }}</span>
      <label for="cf-cfa">Remise CFA</label>
      <InputNumber v-model="form.discount_cfa" input-id="cf-cfa" :min="0" :max="100" :max-fraction-digits="2" suffix=" %" />
      <label for="cf-cfo">Remise CFO</label>
      <InputNumber v-model="form.discount_cfo" input-id="cf-cfo" :min="0" :max="100" :max-fraction-digits="2" suffix=" %" />
      <label>Listes de prix</label>
      <MultiSelect
        v-model="form.price_lists"
        :options="listOptions"
        option-label="label"
        option-value="code"
        filter
        display="chip"
        placeholder="Aucune"
        :virtual-scroller-options="{ itemSize: 38 }"
      />
    </div>
    <template #footer>
      <Button label="Annuler" severity="secondary" text @click="visible = false" />
      <Button label="Enregistrer" icon="pi pi-check" :loading="saving" @click="save" />
    </template>
  </Dialog>
</template>

<style scoped>
.form-grid {
  grid-template-columns: 130px 1fr;
}
</style>
