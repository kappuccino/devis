<script setup lang="ts">
// Export Excel des prix d'un client : choix de la liste (s'il en a plusieurs), contact imprimé
// dans le haut du fichier, puis emplacement du fichier. Le gabarit vient des Réglages.
import { computed, ref, watch } from "vue";
import Dialog from "primevue/dialog";
import Select from "primevue/select";
import InputText from "primevue/inputtext";
import Button from "primevue/button";
import { useToast } from "primevue/usetoast";
import { save } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { api, type Client } from "../api";
import { errorMessage } from "../format";

const visible = defineModel<boolean>("visible", { required: true });
const props = defineProps<{ client: Client | null }>();

const toast = useToast();
const list = ref<string | null>(null);
const contact = ref({ lastName: "", firstName: "", email: "" });
const exporting = ref(false);

const lists = computed(() => props.client?.price_lists ?? []);

watch(visible, (open) => {
  if (!open || !props.client) return;
  list.value = lists.value[0] ?? null;
  // Email du client (le premier s'il y en a plusieurs) ; nom et prénom à saisir.
  contact.value = { lastName: "", firstName: "", email: props.client.email?.split(/[\s;,]+/)[0] ?? "" };
});

/** Nom de fichier proposé : liste et client, sans caractères interdits. */
const fileName = () =>
  `Liste de prix ${list.value} - ${props.client?.name ?? ""}.xlsx`.replace(/[\\/:*?"<>|]/g, "_");

async function exportList() {
  const client = props.client;
  if (!client || !list.value) return;
  const path = await save({
    title: "Enregistrer la liste de prix",
    defaultPath: fileName(),
    filters: [{ name: "Excel", extensions: ["xlsx"] }],
  });
  if (!path) return;
  exporting.value = true;
  try {
    await api.exportPriceList(path, client.code, list.value, contact.value);
    visible.value = false;
    toast.add({ severity: "success", summary: "Liste de prix exportée", detail: path, life: 4000 });
    await openPath(path).catch(() => {});
  } catch (e) {
    toast.add({ severity: "error", summary: "Export Excel", detail: errorMessage(e) });
  } finally {
    exporting.value = false;
  }
}
</script>

<template>
  <Dialog v-model:visible="visible" modal :header="`Exporter les prix — ${client?.name ?? ''}`" :style="{ width: '480px' }">
    <form class="form-grid" @submit.prevent="exportList">
      <label for="export-list">Liste de prix</label>
      <Select
        v-model="list"
        input-id="export-list"
        :options="lists"
        :disabled="lists.length < 2"
        placeholder="Aucune liste de prix"
      />
      <label for="export-last-name">Nom du contact</label>
      <InputText id="export-last-name" v-model="contact.lastName" autofocus />
      <label for="export-first-name">Prénom</label>
      <InputText id="export-first-name" v-model="contact.firstName" />
      <label for="export-email">Email</label>
      <InputText id="export-email" v-model="contact.email" type="email" />
      <button type="submit" hidden />
    </form>
    <p class="muted hint">Le haut du fichier vient du gabarit choisi dans Réglages → Config PDF → Export Excel.</p>
    <template #footer>
      <Button label="Annuler" severity="secondary" text @click="visible = false" />
      <Button label="Exporter…" icon="pi pi-file-excel" :loading="exporting" :disabled="!list" @click="exportList" />
    </template>
  </Dialog>
</template>

<style scoped>
.hint {
  margin-bottom: 0;
  font-size: 0.85em;
}
</style>
