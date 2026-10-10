<script setup lang="ts">
// Réglages → Sauvegarde : dossier des copies de la base des devis, nombre gardé, copie au
// démarrage, sauvegarde immédiate et restauration. Les réglages sont enregistrés dès leur
// modification (pas de bouton « Enregistrer » dans cette rubrique).
import { onMounted, ref } from "vue";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { openFile, pickFolder } from "../dialogs";
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";
import InputNumber from "primevue/inputnumber";
import DataTable from "primevue/datatable";
import Column from "primevue/column";
import Tag from "primevue/tag";
import { useToast } from "primevue/usetoast";
import { useConfirm } from "primevue/useconfirm";
import { api, type BackupFile, type BackupInfo } from "../api";
import { errorMessage } from "../format";

// Clés partagées avec src-tauri/src/backup.rs.
const DIR_KEY = "backup_dir";
const KEEP_KEY = "backup_keep";
const AUTO_KEY = "backup_auto";
const DEFAULT_KEEP = 10;

const toast = useToast();
const confirm = useConfirm();
const info = ref<BackupInfo | null>(null);
const customDir = ref(false);
const keep = ref(DEFAULT_KEEP);
const auto = ref(true);
const saving = ref(false);
const restoring = ref<string | null>(null);

async function refresh() {
  try {
    const [i, settings] = await Promise.all([api.backupInfo(), api.getSettings()]);
    info.value = i;
    customDir.value = !!settings[DIR_KEY]?.trim();
    keep.value = Number(settings[KEEP_KEY]) || DEFAULT_KEEP;
    auto.value = settings[AUTO_KEY] !== "0";
  } catch (e) {
    toast.add({ severity: "error", summary: "Sauvegardes", detail: errorMessage(e) });
  }
}

async function saveSetting(key: string, value: string) {
  try {
    await api.saveSettings({ [key]: value });
    await refresh();
  } catch (e) {
    toast.add({ severity: "error", summary: "Réglage de sauvegarde", detail: errorMessage(e) });
  }
}

async function chooseDir() {
  const dir = await pickFolder({ title: "Dossier des sauvegardes" });
  if (dir) await saveSetting(DIR_KEY, dir);
}

async function backupNow() {
  saving.value = true;
  try {
    const path = await api.backupNow();
    toast.add({ severity: "success", summary: "Base sauvegardée", detail: path, life: 3000 });
    await refresh();
  } catch (e) {
    toast.add({ severity: "error", summary: "Sauvegarde", detail: errorMessage(e) });
  } finally {
    saving.value = false;
  }
}

function restore(file: BackupFile) {
  confirm.require({
    header: "Restaurer cette sauvegarde",
    message:
      `Remplacer toute la base actuelle (devis, clients, listes de prix, réglages) par la copie du ${formatDate(file.modified)} ? ` +
      "La base actuelle est d'abord copiée (« avant restauration ») ; l'appli se recharge ensuite.",
    icon: "pi pi-history",
    rejectProps: { label: "Annuler", severity: "secondary", text: true },
    acceptProps: { label: "Restaurer", severity: "danger" },
    accept: async () => {
      restoring.value = file.path;
      try {
        await api.restoreBackup(file.path);
        // Toutes les données ont changé : on repart d'un écran rechargé.
        window.location.reload();
      } catch (e) {
        toast.add({ severity: "error", summary: "Restauration", detail: errorMessage(e) });
        restoring.value = null;
      }
    },
  });
}

const dateFormat = new Intl.DateTimeFormat("fr-FR", { dateStyle: "short", timeStyle: "short" });
const formatDate = (ms: number | null) => (ms == null ? "—" : dateFormat.format(new Date(ms)));
const formatSize = (bytes: number) =>
  bytes < 1024 * 1024 ? `${Math.max(1, Math.round(bytes / 1024))} Ko` : `${(bytes / 1024 / 1024).toFixed(1).replace(".", ",")} Mo`;

onMounted(refresh);
</script>

<template>
  <section class="card">
    <h2>Sauvegarde de la base des devis</h2>
    <p class="muted">
      Copies de la base (devis, clients, listes de prix, réglages). Un dossier synchronisé (OneDrive, réseau…) protège
      aussi en cas de panne ou de perte de l'ordinateur.
    </p>
    <div class="form-grid">
      <label>Dossier</label>
      <div class="dir-row">
        <code class="dir">{{ info?.dir ?? "…" }}</code>
        <Button label="Choisir…" icon="pi pi-folder-open" severity="secondary" @click="chooseDir" />
        <Button
          v-if="info"
          v-tooltip.bottom="'Ouvrir le dossier'"
          icon="pi pi-external-link"
          text
          rounded
          severity="secondary"
          aria-label="Ouvrir le dossier"
          @click="openFile(info.dir).catch(() => {})"
        />
        <Button
          v-if="customDir"
          v-tooltip.bottom="`Revenir au dossier par défaut (${info?.default_dir ?? ''})`"
          icon="pi pi-undo"
          text
          rounded
          severity="secondary"
          aria-label="Dossier par défaut"
          @click="saveSetting(DIR_KEY, '')"
        />
      </div>
      <label for="backup-keep">Copies gardées</label>
      <div class="inline">
        <InputNumber
          v-model="keep"
          input-id="backup-keep"
          :min="1"
          :max="365"
          show-buttons
          fluid
          class="keep-input"
          @update:model-value="(v: number | null) => v && saveSetting(KEEP_KEY, String(v))"
        />
        <span class="muted">les plus anciennes sont supprimées (sauf les copies « avant restauration »)</span>
      </div>
      <label for="backup-auto">Automatique</label>
      <div class="inline">
        <Checkbox v-model="auto" input-id="backup-auto" binary @update:model-value="(v: boolean) => saveSetting(AUTO_KEY, v ? '1' : '0')" />
        <label for="backup-auto">Sauvegarder au démarrage de l'appli (au plus une fois par jour)</label>
      </div>
    </div>
    <div class="actions">
      <Button label="Sauvegarder maintenant" icon="pi pi-save" :loading="saving" @click="backupNow" />
    </div>
  </section>

  <section class="card">
    <h2>Sauvegardes disponibles</h2>
    <DataTable :value="info?.files ?? []" size="small" striped-rows data-key="path">
      <Column header="Date">
        <template #body="{ data }">
          {{ formatDate(data.modified) }}
          <Tag v-if="data.safety" value="avant restauration" severity="warn" class="safety" />
        </template>
      </Column>
      <Column header="Fichier">
        <template #body="{ data }"><span class="mono muted">{{ data.name }}</span></template>
      </Column>
      <Column header="Taille" class="num" style="width: 90px">
        <template #body="{ data }">{{ formatSize(data.size) }}</template>
      </Column>
      <Column style="width: 170px">
        <template #body="{ data }">
          <div class="row-actions">
            <Button
              v-tooltip.left="'Afficher dans le dossier'"
              icon="pi pi-search"
              text
              rounded
              size="small"
              severity="secondary"
              aria-label="Afficher dans le dossier"
              @click="revealItemInDir(data.path).catch(() => {})"
            />
            <Button
              label="Restaurer"
              icon="pi pi-history"
              text
              size="small"
              severity="danger"
              :loading="restoring === data.path"
              @click="restore(data)"
            />
          </div>
        </template>
      </Column>
      <template #empty><span class="muted">Aucune sauvegarde dans ce dossier.</span></template>
    </DataTable>
  </section>
</template>

<style scoped>
/* Même présentation que les autres dossiers des Réglages. */
.dir-row {
  display: flex;
  gap: 0.75rem;
  align-items: center;
}

.dir {
  flex: 1;
  min-width: 0;
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--app-border);
  border-radius: 6px;
  background: var(--app-bg);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.keep-input {
  width: 7rem;
}

.inline {
  display: flex;
  align-items: center;
  gap: 0.6rem;
}

.inline label {
  color: inherit;
}

.actions {
  margin-top: 1rem;
}

.safety {
  margin-left: 0.5rem;
}

.row-actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.25rem;
}
</style>
