<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useDebouncedSearch } from "../composables/useDebouncedSearch";
import DataTable from "primevue/datatable";
import Column from "primevue/column";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import { useToast } from "primevue/usetoast";
import { useRouter } from "vue-router";
import { api, type Client, type QuoteSummary } from "../api";
import { errorMessage, formatDate, formatEuro, formatPct } from "../format";
import Button from "primevue/button";
import ClientFormDialog from "../components/ClientFormDialog.vue";
import PriceListExportDialog from "../components/PriceListExportDialog.vue";

const toast = useToast();
const router = useRouter();
const clients = ref<Client[]>([]);
/** Devis de chaque client (par code), du plus récent au plus ancien ; versions comprises. */
const quotesByClient = ref(new Map<string, QuoteSummary[]>());
/** Clients dépliés (code → true) : leurs devis s'affichent sous la ligne. */
const expanded = ref<Record<string, boolean>>({});
const loading = ref(true);
const search = ref("");
const query = useDebouncedSearch(search);

const dialogVisible = ref(false);
const editing = ref<Client | null>(null);

/** Clients avec leur nombre de devis (colonne triable). */
const rows = computed(() => clients.value.map((c) => ({ ...c, quote_count: quotesByClient.value.get(c.code)?.length ?? 0 })));

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return rows.value;
  return rows.value.filter((c) =>
    [c.code, c.name, c.group_name, c.subgroup, c.sales_rep, c.siren, c.comment].some((v) =>
      v?.toLowerCase().includes(q),
    ),
  );
});

async function load() {
  loading.value = true;
  try {
    const [list, quotes] = await Promise.all([api.listClients(), api.listQuotes()]);
    clients.value = list;
    const byClient = new Map<string, QuoteSummary[]>();
    // Liste déjà triée du plus récent au plus ancien.
    for (const q of quotes) {
      if (q.client_code) byClient.set(q.client_code, [...(byClient.get(q.client_code) ?? []), q]);
    }
    quotesByClient.value = byClient;
  } catch (e) {
    toast.add({ severity: "error", summary: "Chargement des clients", detail: errorMessage(e) });
  } finally {
    loading.value = false;
  }
}

function toggleQuotes(code: string) {
  const next = { ...expanded.value };
  if (next[code]) delete next[code];
  else next[code] = true;
  expanded.value = next;
}

const openQuote = (q: QuoteSummary) => router.push(`/devis/${q.id}`);

function openClient(client: Client) {
  editing.value = client;
  dialogVisible.value = true;
}

/** Export Excel des prix du client. */
const exportVisible = ref(false);
const exporting = ref<Client | null>(null);
function openExport(client: Client) {
  exporting.value = client;
  exportVisible.value = true;
}

function onSaved(saved: Client) {
  const i = clients.value.findIndex((c) => c.code === saved.code);
  if (i >= 0) clients.value[i] = saved;
}

onMounted(load);
</script>

<template>
  <div class="page">
    <div class="page-header">
      <h1>Clients</h1>
      <span class="muted">{{ filtered.length }} / {{ clients.length }}</span>
      <span class="spacer" />
      <InputText v-model="search" placeholder="Nom, code, groupe, commercial, commentaire…" style="width: 280px" />
    </div>
    <DataTable
      :value="filtered"
      :loading="loading"
      paginator
      :rows="200"
      :rows-per-page-options="[50, 100, 200, 500]"
      size="small"
      striped-rows
      selection-mode="single"
      data-key="code"
      :expanded-rows="expanded"
      class="card"
      @row-click="openClient($event.data)"
    >
      <!-- Flèche : déplie les devis du client (seulement s'il en a). -->
      <Column style="width: 36px" class="expander-cell">
        <template #body="{ data }">
          <Button
            v-if="data.quote_count"
            :icon="expanded[data.code] ? 'pi pi-chevron-down' : 'pi pi-chevron-right'"
            text
            rounded
            size="small"
            severity="secondary"
            :aria-label="expanded[data.code] ? 'Replier les devis' : 'Déplier les devis'"
            @click.stop="toggleQuotes(data.code)"
          />
        </template>
      </Column>
      <Column field="code" header="Code" sortable>
        <template #body="{ data }">
          <span class="mono">{{ data.code }}</span>
          <Tag v-if="data.is_one_off" value="Ponctuel" severity="info" class="one-off-tag" />
        </template>
      </Column>
      <Column field="name" header="Raison sociale" sortable>
        <template #body="{ data }">
          {{ data.name }}
          <i
            v-if="data.comment"
            v-tooltip.top="{ value: data.comment, class: 'comment-tooltip' }"
            class="pi pi-comment comment-icon"
            aria-label="Commentaire"
          />
        </template>
      </Column>
      <Column field="group_name" header="Groupe" sortable />
      <Column field="sales_rep" header="Commercial" sortable />
      <Column field="discount_cfa" header="Remise CFA" sortable class="num">
        <template #body="{ data }">{{ formatPct(data.discount_cfa) }}</template>
      </Column>
      <Column field="discount_cfo" header="Remise CFO" sortable class="num">
        <template #body="{ data }">{{ formatPct(data.discount_cfo) }}</template>
      </Column>
      <Column field="quote_count" header="Devis" sortable class="num" style="width: 80px">
        <template #body="{ data }">
          <span :class="{ muted: !data.quote_count }">{{ data.quote_count || "–" }}</span>
        </template>
      </Column>
      <Column header="Listes de prix">
        <template #body="{ data }">
          <div class="tags">
            <Tag v-for="l in data.price_lists" :key="l" :value="l" severity="secondary" />
          </div>
        </template>
      </Column>
      <Column style="width: 52px">
        <template #body="{ data }">
          <Button
            v-tooltip.left="data.price_lists.length ? 'Exporter ses prix (Excel)' : 'Aucune liste de prix'"
            icon="pi pi-file-excel"
            text
            rounded
            size="small"
            severity="secondary"
            :disabled="!data.price_lists.length"
            aria-label="Exporter ses prix (Excel)"
            @click.stop="openExport(data)"
          />
        </template>
      </Column>
      <template #expansion="{ data }">
        <div class="client-quotes">
          <table>
            <thead>
              <tr>
                <th>N°</th>
                <th>Date</th>
                <th>Affaire</th>
                <th class="num">Total HT</th>
                <th>Obtenue</th>
                <th>Commentaire</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="q in quotesByClient.get(data.code) ?? []"
                :key="q.id"
                v-tooltip.left="'Ouvrir le devis'"
                :class="{ superseded: q.superseded }"
                @click="openQuote(q)"
              >
                <td>
                  <span class="mono link">{{ q.number }}</span>
                  <span v-if="q.superseded" class="muted old">ancienne version</span>
                </td>
                <td>{{ formatDate(q.date) }}</td>
                <td>{{ q.project_name }}</td>
                <td class="num">
                  <strong>{{ formatEuro(q.total_net) }}</strong>
                </td>
                <td>
                  <i
                    v-if="q.won_at"
                    v-tooltip.top="`Affaire obtenue (cochée le ${formatDate(q.won_at)})`"
                    class="pi pi-check-circle won"
                  />
                </td>
                <td class="muted">{{ q.comment }}</td>
              </tr>
            </tbody>
          </table>
          <Button
            label="Voir dans la liste des devis"
            icon="pi pi-arrow-right"
            icon-pos="right"
            text
            size="small"
            @click="router.push({ path: '/devis', query: { client: data.code } })"
          />
        </div>
      </template>
      <template #empty>
        <span class="muted">Aucun client. Importez le fichier LPN depuis les Réglages.</span>
      </template>
    </DataTable>

    <ClientFormDialog v-model:visible="dialogVisible" :client="editing" @saved="onSaved" />
    <PriceListExportDialog v-model:visible="exportVisible" :client="exporting" />
  </div>
</template>

<style scoped>
.tags {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}

.one-off-tag {
  margin-left: 0.4rem;
  font-size: 0.7rem;
  padding: 0.1rem 0.4rem;
}

.expander-cell :deep(.p-button) {
  width: 1.75rem;
  height: 1.75rem;
}

/* Devis d'un client, dépliés sous sa ligne. */
.client-quotes {
  padding: 0.25rem 0 0.5rem 2.5rem;
}

.client-quotes table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.9em;
}

.client-quotes th {
  text-align: left;
  font-weight: 600;
  color: var(--app-muted);
  padding: 0.3rem 0.6rem;
  border-bottom: 1px solid var(--app-border);
}

.client-quotes td {
  padding: 0.3rem 0.6rem;
  border-bottom: 1px solid var(--app-border);
}

.client-quotes .num {
  text-align: right;
}

.client-quotes tbody tr {
  cursor: pointer;
}

.client-quotes tbody tr:hover {
  background: color-mix(in srgb, var(--app-accent) 6%, transparent);
}

.client-quotes tr.superseded td {
  opacity: 0.6;
}

.client-quotes .link {
  color: var(--app-accent);
}

.client-quotes .old {
  margin-left: 0.4rem;
  font-size: 0.85em;
}

.client-quotes .won {
  color: var(--app-success);
}

.comment-icon {
  margin-left: 0.35rem;
  color: var(--app-muted);
  font-size: 0.85rem;
}
</style>
