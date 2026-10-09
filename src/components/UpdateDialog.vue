<script setup lang="ts">
import { computed } from "vue";
import Dialog from "primevue/dialog";
import Button from "primevue/button";
import ProgressBar from "primevue/progressbar";
import Message from "primevue/message";
import { installUpdate, updater } from "../updater";

/** Fermée par l'utilisateur : on ne la rouvre pas avant le prochain lancement. */
const dismissed = defineModel<boolean>("dismissed", { default: false });

const visible = computed({
  get: () => !!updater.available && !dismissed.value,
  set: (v: boolean) => {
    if (!v && !updater.installing) dismissed.value = true;
  },
});
</script>

<template>
  <Dialog v-model:visible="visible" modal header="Mise à jour disponible" :style="{ width: '460px' }" :closable="!updater.installing">
    <p>
      La version <strong>{{ updater.available?.version }}</strong> est disponible
      (version installée : {{ updater.available?.currentVersion }}).
    </p>
    <p v-if="updater.available?.body" class="notes muted">{{ updater.available.body }}</p>
    <template v-if="updater.installing">
      <ProgressBar :value="updater.progress ?? undefined" :mode="updater.progress == null ? 'indeterminate' : 'determinate'" style="height: 8px" />
      <p class="muted">Téléchargement puis installation ; l'application redémarre ensuite.</p>
    </template>
    <Message v-if="updater.error" severity="error" :closable="false">{{ updater.error }}</Message>
    <template #footer>
      <Button label="Plus tard" severity="secondary" text :disabled="updater.installing" @click="visible = false" />
      <Button label="Installer et redémarrer" icon="pi pi-download" :loading="updater.installing" @click="installUpdate" />
    </template>
  </Dialog>
</template>

<style scoped>
.notes {
  white-space: pre-line;
  max-height: 200px;
  overflow: auto;
  font-size: 0.9em;
}
</style>
