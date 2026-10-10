<script setup lang="ts">
// Tiroir latéral : clients rattachés à une liste de prix (recherche, détacher) et rattachement
// d'un autre client de la base. Ouvert depuis l'écran Listes de prix.
import { computed, ref, watch } from "vue";
import Drawer from "primevue/drawer";
import DataTable from "primevue/datatable";
import Column from "primevue/column";
import InputText from "primevue/inputtext";
import IconField from "primevue/iconfield";
import InputIcon from "primevue/inputicon";
import AutoComplete, { type AutoCompleteCompleteEvent } from "primevue/autocomplete";
import Button from "primevue/button";
import { useToast } from "primevue/usetoast";
import { api, type ClientRef } from "../api";
import { errorMessage } from "../format";

/** Code de la liste affichée ; null : tiroir fermé. */
const code = defineModel<string | null>("code", { required: true });
/** Après un rattachement ou un détachement : le parent recharge ses compteurs. */
const emit = defineEmits<{ changed: [] }>();

const toast = useToast();
const attached = ref<ClientRef[]>([]);
const allClients = ref<ClientRef[]>([]);
const loading = ref(false);
const search = ref("");
const busy = ref<string | null>(null);

const visible = computed({
  get: () => code.value != null,
  set: (v) => {
    if (!v) code.value = null;
  },
});

const matches = (c: ClientRef, q: string) => c.name.toLowerCase().includes(q) || c.code.toLowerCase().includes(q);

const filtered = computed(() => {
  const q = search.value.trim().toLowerCase();
  return q ? attached.value.filter((c) => matches(c, q)) : attached.value;
});

// Rattacher : recherche parmi les clients pas encore rattachés.
const toAttach = ref<ClientRef | string | null>(null);
const suggestions = ref<ClientRef[]>([]);
function searchClients(e: AutoCompleteCompleteEvent) {
  const q = e.query.trim().toLowerCase();
  const already = new Set(attached.value.map((c) => c.code));
  suggestions.value = allClients.value.filter((c) => !already.has(c.code) && matches(c, q)).slice(0, 50);
}

async function attach(client: ClientRef) {
  if (!code.value) return;
  busy.value = client.code;
  try {
    await api.setClientPriceList(client.code, code.value, true);
    attached.value = [...attached.value, client].sort((a, b) => a.name.localeCompare(b.name));
    toAttach.value = null;
    toast.add({ severity: "success", summary: `${client.name} rattaché à ${code.value}`, life: 2000 });
    emit("changed");
  } catch (e) {
    toast.add({ severity: "error", summary: "Rattachement", detail: errorMessage(e) });
  } finally {
    busy.value = null;
  }
}

async function detach(client: ClientRef) {
  if (!code.value) return;
  busy.value = client.code;
  try {
    await api.setClientPriceList(client.code, code.value, false);
    attached.value = attached.value.filter((c) => c.code !== client.code);
    toast.add({ severity: "info", summary: `${client.name} détaché de ${code.value}`, life: 2000 });
    emit("changed");
  } catch (e) {
    toast.add({ severity: "error", summary: "Détachement", detail: errorMessage(e) });
  } finally {
    busy.value = null;
  }
}

watch(code, async (c) => {
  if (!c) return;
  search.value = "";
  toAttach.value = null;
  attached.value = [];
  loading.value = true;
  try {
    const [rows, clients] = await Promise.all([api.getPriceListClients(c), api.listClients()]);
    if (code.value !== c) return;
    attached.value = rows;
    allClients.value = clients.map(({ code, name }) => ({ code, name }));
  } catch (e) {
    toast.add({ severity: "error", summary: `Clients de la liste ${c}`, detail: errorMessage(e) });
  } finally {
    loading.value = false;
  }
});
</script>

<template>
  <Drawer v-model:visible="visible" position="right" class="list-clients-drawer" :style="{ width: 'min(560px, 92vw)' }">
    <template #header>
      <div class="drawer-title">
        <h2>Clients de la liste <span class="mono">{{ code }}</span></h2>
        <span class="muted">{{ loading ? "Chargement…" : `${attached.length} client(s) rattaché(s)` }}</span>
      </div>
    </template>

    <div class="attach">
      <AutoComplete
        v-model="toAttach"
        :suggestions="suggestions"
        option-label="name"
        placeholder="Rattacher un client (nom ou code)…"
        :min-length="2"
        :delay="150"
        force-selection
        fluid
        :disabled="loading"
        @complete="searchClients"
        @option-select="(e: { value: ClientRef }) => attach(e.value)"
      >
        <template #option="{ option }">
          <div class="opt">
            <span>{{ option.name }}</span>
            <span class="muted mono">{{ option.code }}</span>
          </div>
        </template>
        <template #empty><span class="muted">Aucun client à rattacher</span></template>
      </AutoComplete>
    </div>

    <IconField>
      <InputIcon class="pi pi-search" />
      <InputText v-model="search" placeholder="Filtrer les clients rattachés…" fluid />
    </IconField>

    <DataTable
      :value="filtered"
      :loading="loading"
      size="small"
      striped-rows
      scrollable
      scroll-height="flex"
      data-key="code"
      class="clients"
    >
      <Column field="name" header="Client" sortable />
      <Column field="code" header="Code" sortable style="width: 110px">
        <template #body="{ data }"><span class="mono">{{ data.code }}</span></template>
      </Column>
      <Column style="width: 48px">
        <template #body="{ data }">
          <Button
            v-tooltip.left="'Détacher de cette liste'"
            icon="pi pi-times"
            text
            rounded
            size="small"
            severity="secondary"
            :loading="busy === data.code"
            :aria-label="`Détacher ${data.name}`"
            @click="detach(data)"
          />
        </template>
      </Column>
      <template #empty>
        <span class="muted">{{ search ? "Aucun client ne correspond." : "Aucun client rattaché." }}</span>
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

.attach {
  margin-bottom: 0.75rem;
}

.opt {
  display: flex;
  gap: 0.75rem;
  justify-content: space-between;
  width: 100%;
}

/* Le tableau prend la hauteur restante, puis défile. */
.clients {
  flex: 1;
  min-height: 0;
  margin-top: 0.75rem;
}
</style>

<style>
.p-drawer.list-clients-drawer .p-drawer-content {
  display: flex;
  flex-direction: column;
}
</style>
