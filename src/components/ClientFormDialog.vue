<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Dialog from "primevue/dialog";
import InputNumber from "primevue/inputnumber";
import InputText from "primevue/inputtext";
import Textarea from "primevue/textarea";
import Tag from "primevue/tag";
import MultiSelect from "primevue/multiselect";
import Button from "primevue/button";
import { useToast } from "primevue/usetoast";
import { api, type Client, type PriceList } from "../api";
import { errorMessage } from "../format";

/**
 * Fiche d'un client de la base : remises, listes de prix et commentaire. Le reste vient du fichier
 * Excel, sauf pour un client ponctuel (ajouté depuis un devis), dont la raison sociale et le n° client
 * se modifient ici (son vrai n° MFG/PRO, ou un n° provisoire ; ses devis suivent).
 * Ouverte depuis la liste des clients et depuis un devis.
 */
const props = defineProps<{ client: Client | null }>();
const visible = defineModel<boolean>("visible", { required: true });
/** previousCode : code avant l'enregistrement (différent de client.code si le n° a été changé). */
const emit = defineEmits<{ saved: [client: Client, previousCode: string] }>();

const toast = useToast();
const priceLists = ref<PriceList[]>([]);
const saving = ref(false);
const form = ref({ code: "", name: "", discount_cfa: 0, discount_cfo: 0, price_lists: [] as string[], comment: "" });

const listOptions = computed(() =>
  priceLists.value.map((l) => ({ code: l.code, label: `${l.code}${l.label ? ` — ${l.label}` : ""} (${l.item_count})` })),
);

watch(visible, async (open) => {
  const c = props.client;
  if (!open || !c) return;
  form.value = {
    code: c.code,
    name: c.name,
    discount_cfa: c.discount_cfa,
    discount_cfo: c.discount_cfo,
    price_lists: [...c.price_lists],
    comment: c.comment,
  };
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
    const f = form.value;
    if (c.is_one_off && !f.name.trim()) {
      toast.add({ severity: "warn", summary: "Indiquez la raison sociale", life: 2500 });
      return;
    }
    const code = c.is_one_off ? f.code.trim().toUpperCase() : c.code;
    if (!code) {
      toast.add({ severity: "warn", summary: "Indiquez le n° client", life: 2500 });
      return;
    }
    const saved: Client = {
      ...c,
      code,
      name: c.is_one_off ? f.name.trim() : c.name,
      discount_cfa: f.discount_cfa ?? 0,
      discount_cfo: f.discount_cfo ?? 0,
      price_lists: [...f.price_lists],
      comment: f.comment.trimEnd(),
    };
    await api.updateClient({ ...saved, code: c.code }, code);
    toast.add({
      severity: "success",
      summary: "Client enregistré",
      detail: code !== c.code ? `N° ${c.code} remplacé par ${code}, devis compris` : undefined,
      life: code !== c.code ? 4000 : 2000,
    });
    emit("saved", saved, c.code);
    visible.value = false;
  } catch (e) {
    toast.add({ severity: "error", summary: "Enregistrement", detail: errorMessage(e) });
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <Dialog v-model:visible="visible" modal :style="{ width: '560px' }" :draggable="false">
    <template #header>
      <div class="cf-header">
        <span class="p-dialog-title">{{ client?.name }}</span>
        <Tag v-if="client?.is_one_off" value="Ponctuel" severity="info" />
      </div>
    </template>
    <div v-if="client" class="form-grid">
      <template v-if="client.is_one_off">
        <label for="cf-code">N° client</label>
        <InputText
          id="cf-code"
          v-model="form.code"
          v-tooltip.bottom="'Son vrai n° (MFG/PRO) ou un n° provisoire. Un n° commençant par P n\'est pas imprimé sur le PDF.'"
          class="mono"
        />
        <label for="cf-name">Raison sociale</label>
        <InputText id="cf-name" v-model="form.name" />
      </template>
      <template v-else>
        <label>Code</label><span class="mono">{{ client.code }}</span>
      </template>
      <template v-if="!client.is_one_off || client.group_name">
        <label>Groupe</label><span>{{ client.group_name }} {{ client.subgroup ? `/ ${client.subgroup}` : "" }}</span>
      </template>
      <template v-if="!client.is_one_off || client.sales_rep">
        <label>Commercial</label><span>{{ client.sales_rep }}</span>
      </template>
      <template v-if="!client.is_one_off || client.email">
        <label>Email</label><span>{{ client.email }}</span>
      </template>
      <template v-if="!client.is_one_off || client.siren">
        <label>SIREN</label><span>{{ client.siren }}</span>
      </template>
      <template v-if="!client.is_one_off || client.franco">
        <label>Franco</label><span>{{ client.franco }}</span>
      </template>
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
      <label for="cf-comment" class="cf-comment-label">Commentaire</label>
      <Textarea
        id="cf-comment"
        v-model="form.comment"
        rows="4"
        auto-resize
        placeholder="Annotations sur le client (non imprimées, gardées à l'import LPN)"
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

.cf-header {
  display: flex;
  align-items: center;
  gap: 0.6rem;
}

.cf-comment-label {
  align-self: start;
  padding-top: 0.5rem;
}

#cf-comment {
  max-height: 320px;
}
</style>
