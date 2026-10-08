<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Textarea from "primevue/textarea";
import Message from "primevue/message";
import ProgressBar from "primevue/progressbar";
import Select from "primevue/select";
import DataTable from "primevue/datatable";
import Column from "primevue/column";
import { useToast } from "primevue/usetoast";
import { useConfirm } from "primevue/useconfirm";
import { api, type DbStats, type ImportReport, type PriceList, type Settings } from "../api";
import { FAVORITE_LISTS_KEY, favoriteLists } from "../favorites";
import { errorMessage, formatNumber } from "../format";

/** Rubrique affichée : import, favoris ou pdf (dans l'adresse, /reglages/<rubrique>). */
const props = defineProps<{ section: string }>();

const sections = [
  { key: "import", label: "Import de données", hint: "Fichier LPN, statistiques", icon: "pi pi-upload" },
  { key: "favoris", label: "Favoris", hint: "Listes de prix favorites", icon: "pi pi-star" },
  { key: "pdf", label: "Config PDF", hint: "Société, numérotation", icon: "pi pi-file-pdf" },
];
const current = computed(() => sections.find((s) => s.key === props.section) ?? sections[0]);

const toast = useToast();
const confirm = useConfirm();
const stats = ref<DbStats | null>(null);
const settings = ref<Settings>({});
const importing = ref(false);
const report = ref<ImportReport | null>(null);
const saving = ref(false);

// Listes de prix favorites : proposées dans les devis pour forcer une liste. Enregistrées dès le choix.
const allPriceLists = ref<PriceList[]>([]);
/** Listes pas encore favorites, pour l'ajout. */
const addOptions = computed(() =>
  allPriceLists.value
    .filter((l) => !favorites.value.includes(l.code))
    .map((l) => ({ code: l.code, label: `${l.code}${l.label ? ` — ${l.label}` : ""} (${l.item_count})` })),
);
/** Tableau des favorites : code, nom et nombre de produits de la liste. */
const favoriteRows = computed(() =>
  favorites.value.map((code) => {
    const list = allPriceLists.value.find((l) => l.code === code);
    return { code, label: list?.label ?? null, item_count: list?.item_count ?? null };
  }),
);
const toAdd = ref<string | null>(null);

function addFavorite(code: string | null) {
  if (code && !favorites.value.includes(code)) favorites.value = [...favorites.value, code];
  toAdd.value = null;
}

function removeFavorite(code: string) {
  favorites.value = favorites.value.filter((c) => c !== code);
}
const favorites = computed({
  get: () => favoriteLists(settings.value),
  set: async (codes: string[]) => {
    const value = codes.join(",");
    settings.value[FAVORITE_LISTS_KEY] = value;
    try {
      await api.saveSettings({ [FAVORITE_LISTS_KEY]: value });
    } catch (e) {
      toast.add({ severity: "error", summary: "Listes favorites", detail: errorMessage(e) });
    }
  },
});

type Field = { key: string; label: string; multiline?: boolean; placeholder?: string };
/** Champs imprimés sur le PDF : en-tête société, puis informations du devis. */
const companyFields: Field[] = [
  { key: "company_name", label: "Raison sociale" },
  { key: "company_address", label: "Adresse", multiline: true },
  { key: "company_phone", label: "Téléphone" },
  { key: "company_email", label: "Email" },
  { key: "company_siret", label: "SIRET" },
];
const quoteFields: Field[] = [
  { key: "quote_prefix", label: "Préfixe des n° de devis", placeholder: "DEV → DEV-2026-0001" },
  { key: "quote_validity", label: "Validité", placeholder: "ex. 30 jours" },
  { key: "quote_conditions", label: "Conditions (bas de devis)", multiline: true },
];

async function loadStats() {
  stats.value = await api.dbStats();
}

async function pickAndImport() {
  const path = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "Excel", extensions: ["xlsx", "xlsm"] }],
  });
  if (!path) return;
  confirm.require({
    header: "Remplacer les données",
    message:
      "Les produits, clients et listes de prix seront remplacés par le contenu du fichier " +
      "(y compris les remises et listes modifiées à la main). Les devis existants sont conservés.",
    icon: "pi pi-exclamation-triangle",
    rejectProps: { label: "Annuler", severity: "secondary", text: true },
    acceptProps: { label: "Importer" },
    accept: () => runImport(path),
  });
}

async function runImport(path: string) {
  importing.value = true;
  report.value = null;
  try {
    report.value = await api.importLpn(path);
    toast.add({ severity: "success", summary: "Import terminé", life: 3000 });
    await loadStats();
  } catch (e) {
    toast.add({ severity: "error", summary: "Import", detail: errorMessage(e) });
  } finally {
    importing.value = false;
  }
}

function pickLogo(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0];
  if (!file) return;
  const reader = new FileReader();
  reader.onload = () => (settings.value.company_logo = String(reader.result));
  reader.readAsDataURL(file);
}

async function saveSettings() {
  saving.value = true;
  try {
    // Seulement les champs du PDF : les favoris sont enregistrés à part, dès leur choix.
    const keys = [...companyFields, ...quoteFields].map((f) => f.key).concat("company_logo");
    await api.saveSettings(Object.fromEntries(keys.map((k) => [k, settings.value[k] ?? ""])));
    toast.add({ severity: "success", summary: "Réglages enregistrés", life: 2000 });
  } catch (e) {
    toast.add({ severity: "error", summary: "Enregistrement", detail: errorMessage(e) });
  } finally {
    saving.value = false;
  }
}

onMounted(async () => {
  try {
    [settings.value, allPriceLists.value] = await Promise.all([api.getSettings(), api.listPriceLists(), loadStats()]);
  } catch (e) {
    toast.add({ severity: "error", summary: "Chargement des réglages", detail: errorMessage(e) });
  }
});
</script>

<template>
  <div class="settings">
    <!-- Sous-menu des réglages -->
    <nav class="subnav">
      <h1>Réglages</h1>
      <RouterLink
        v-for="sec in sections"
        :key="sec.key"
        :to="`/reglages/${sec.key}`"
        class="subnav-link"
        :class="{ active: current.key === sec.key }"
      >
        <i :class="sec.icon" />
        <span>
          <span class="subnav-label">{{ sec.label }}</span>
          <span class="subnav-hint">{{ sec.hint }}</span>
        </span>
      </RouterLink>
    </nav>

    <div class="page">
      <div class="page-header"><h1>{{ current.label }}</h1></div>

      <!-- Import de données -->
      <template v-if="current.key === 'import'">
        <section class="card">
          <h2>Base de données</h2>
          <div v-if="stats" class="stats">
            <div><strong>{{ formatNumber(stats.products) }}</strong><span class="muted">produits</span></div>
            <div><strong>{{ formatNumber(stats.clients) }}</strong><span class="muted">clients</span></div>
            <div><strong>{{ formatNumber(stats.price_lists) }}</strong><span class="muted">listes de prix</span></div>
            <div><strong>{{ formatNumber(stats.price_list_items) }}</strong><span class="muted">prix nets</span></div>
            <div><strong>{{ formatNumber(stats.quotes) }}</strong><span class="muted">devis</span></div>
          </div>
          <p class="muted">
            Base : <span class="mono">{{ stats?.path }}</span>
          </p>
        </section>

        <section class="card">
          <h2>Importer le fichier LPN</h2>
          <p class="muted">
            Classeur « LPN finale.xlsx » : feuilles <em>DATA</em> (produits), <em>gestion tarifs</em>
            (clients, remises CFA/CFO, listes rattachées) et <em>fichier injection tarif</em> (listes de prix nets).
            Produits, clients et listes de prix sont remplacés ; les devis sont conservés.
          </p>
          <Button
            label="Choisir le fichier…"
            icon="pi pi-upload"
            :loading="importing"
            :disabled="importing"
            @click="pickAndImport"
          />
          <ProgressBar v-if="importing" mode="indeterminate" style="height: 4px; margin-top: 0.75rem" />
          <div v-if="report" class="report">
            <Message severity="success" :closable="false">
              {{ formatNumber(report.products) }} produits, {{ formatNumber(report.clients) }} clients,
              {{ formatNumber(report.price_lists) }} listes ({{ formatNumber(report.price_list_items) }} prix),
              {{ formatNumber(report.client_links) }} rattachements client → liste.
            </Message>
            <Message v-for="w in report.warnings" :key="w" severity="warn" :closable="false">{{ w }}</Message>
          </div>
        </section>
      </template>

      <!-- Favoris -->
      <section v-else-if="current.key === 'favoris'" class="card">
        <h2>Listes de prix favorites</h2>
        <p class="muted">
          Proposées dans chaque devis pour <strong>forcer</strong> une liste de prix : elle passe alors avant
          les listes du client. Retirer une favorite ne change pas les devis qui l'utilisent déjà.
          Enregistré immédiatement.
        </p>
        <div class="favorites">
          <Select
            v-model="toAdd"
            :options="addOptions"
            option-label="label"
            option-value="code"
            filter
            placeholder="Ajouter une liste de prix…"
            class="add-favorite"
            :virtual-scroller-options="{ itemSize: 36 }"
            @change="addFavorite(toAdd)"
          />
          <DataTable :value="favoriteRows" size="small" striped-rows data-key="code">
            <Column field="code" header="Liste">
              <template #body="{ data }"><span class="mono">{{ data.code }}</span></template>
            </Column>
            <Column field="label" header="Nom">
              <template #body="{ data }">
                <span v-if="data.label">{{ data.label }}</span>
                <span v-else-if="data.item_count == null" class="muted">absente du dernier import</span>
              </template>
            </Column>
            <Column field="item_count" header="Produits" class="num">
              <template #body="{ data }">{{ formatNumber(data.item_count) }}</template>
            </Column>
            <Column style="width: 48px">
              <template #body="{ data }">
                <Button
                  v-tooltip.left="'Retirer des favorites'"
                  icon="pi pi-times"
                  text
                  rounded
                  size="small"
                  severity="secondary"
                  :aria-label="`Retirer ${data.code} des favorites`"
                  @click="removeFavorite(data.code)"
                />
              </template>
            </Column>
            <template #empty><span class="muted">Aucune liste favorite.</span></template>
          </DataTable>
        </div>
      </section>

      <!-- Config PDF -->
      <template v-else>
        <section class="card">
          <h2>Société (en-tête du PDF)</h2>
          <div class="form-grid">
            <template v-for="f in companyFields" :key="f.key">
              <label :for="f.key">{{ f.label }}</label>
              <Textarea v-if="f.multiline" :id="f.key" v-model="settings[f.key]" rows="3" auto-resize />
              <InputText v-else :id="f.key" v-model="settings[f.key]" :placeholder="f.placeholder" />
            </template>
            <label for="logo">Logo</label>
            <div class="logo">
              <img v-if="settings.company_logo" :src="settings.company_logo" alt="Logo" />
              <input id="logo" type="file" accept="image/png,image/jpeg" @change="pickLogo" />
              <Button
                v-if="settings.company_logo"
                label="Retirer"
                text
                size="small"
                severity="secondary"
                @click="settings.company_logo = ''"
              />
            </div>
          </div>
        </section>

        <section class="card">
          <h2>Devis</h2>
          <div class="form-grid">
            <template v-for="f in quoteFields" :key="f.key">
              <label :for="f.key">{{ f.label }}</label>
              <Textarea v-if="f.multiline" :id="f.key" v-model="settings[f.key]" rows="3" auto-resize />
              <InputText v-else :id="f.key" v-model="settings[f.key]" :placeholder="f.placeholder" />
            </template>
          </div>
        </section>

        <div>
          <Button label="Enregistrer" icon="pi pi-check" :loading="saving" @click="saveSettings" />
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.settings {
  display: grid;
  grid-template-columns: 210px 1fr;
  min-height: 100%;
}

/* Sous-menu : colonne étroite entre la barre latérale et le contenu. */
.subnav {
  background: var(--app-surface);
  border-right: 1px solid var(--app-border);
  padding: 1.25rem 0.6rem;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.subnav h1 {
  font-size: 1.1rem;
  padding: 0 0.7rem 0.9rem;
}

.subnav-link {
  display: flex;
  align-items: flex-start;
  gap: 0.7rem;
  padding: 0.55rem 0.7rem;
  border-radius: 8px;
  color: var(--app-text);
  text-decoration: none;
}

.subnav-link i {
  margin-top: 3px;
  color: var(--app-muted);
}

.subnav-link:hover {
  background: var(--app-bg);
}

.subnav-link.active {
  background: color-mix(in srgb, var(--app-accent) 10%, transparent);
}

.subnav-link.active i,
.subnav-link.active .subnav-label {
  color: var(--app-accent);
}

.subnav-label {
  display: block;
  font-weight: 600;
}

.subnav-hint {
  display: block;
  font-size: 0.82em;
  color: var(--app-muted);
}

.stats {
  display: flex;
  gap: 2rem;
  margin-bottom: 0.5rem;
}

.stats div {
  display: flex;
  flex-direction: column;
}

.stats strong {
  font-size: 1.4rem;
}

.report {
  margin-top: 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.form-grid {
  max-width: 760px;
}

.favorites {
  max-width: 760px;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.add-favorite {
  width: 100%;
}

.logo {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.logo img {
  max-height: 56px;
  max-width: 160px;
}

</style>
