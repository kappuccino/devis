<script setup lang="ts">
// Évolution du chiffrage d'une référence dans tous les devis : courbe du prix net devisé,
// chiffres clés et tableau des devis, avec un filtre sur un ou plusieurs clients.
// Prix net : prix de la ligne après sa remise, puis après la remise globale du devis (sauf option).
import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import Button from "primevue/button";
import MultiSelect from "primevue/multiselect";
import Checkbox from "primevue/checkbox";
import DataTable from "primevue/datatable";
import Column from "primevue/column";
import Tag from "primevue/tag";
import { useToast } from "primevue/usetoast";
import { api, type HistoryLine, type ProductHistory } from "../api";
import { errorMessage, formatDate, formatNumber, formatPct, formatUnitPrice } from "../format";

const props = defineProps<{ productRef: string }>();
const router = useRouter();
const toast = useToast();

const history = ref<ProductHistory | null>(null);
const loading = ref(true);
/** Clients retenus (clé : code, ou « nom:… » pour un client ponctuel) ; vide : tous. */
const selectedClients = ref<string[]>([]);
const withOptions = ref(false);

const clientKey = (l: HistoryLine) => (l.client_code ? l.client_code : `nom:${l.client_name}`);
const clientLabel = (l: HistoryLine) => (l.client_code ? `${l.client_name} (${l.client_code})` : `${l.client_name} (ponctuel)`);

async function load() {
  loading.value = true;
  selectedClients.value = [];
  try {
    history.value = await api.productPriceHistory(props.productRef);
  } catch (e) {
    toast.add({ severity: "error", summary: "Évolution du chiffrage", detail: errorMessage(e) });
  } finally {
    loading.value = false;
  }
}
onMounted(load);
watch(() => props.productRef, load);

/** Clients ayant eu cette référence dans un devis (nombre de devis), les plus fréquents d'abord. */
const clientOptions = computed(() => {
  const byKey = new Map<string, { value: string; label: string; count: number }>();
  for (const l of history.value?.lines ?? []) {
    const k = clientKey(l);
    const o = byKey.get(k) ?? { value: k, label: clientLabel(l), count: 0 };
    o.count++;
    byKey.set(k, o);
  }
  return [...byKey.values()].sort((a, b) => b.count - a.count || a.label.localeCompare(b.label));
});

const lines = computed(() =>
  (history.value?.lines ?? []).filter(
    (l) =>
      (withOptions.value || !l.is_option) &&
      (!selectedClients.value.length || selectedClients.value.includes(clientKey(l))),
  ),
);

// ---------- Chiffres clés ----------
const kpis = computed(() => {
  const ls = lines.value;
  if (!ls.length) return null;
  const quantity = ls.reduce((s, l) => s + l.quantity, 0);
  const prices = ls.map((l) => l.net_unit_price);
  const weighted = quantity ? ls.reduce((s, l) => s + l.net_unit_price * l.quantity, 0) / quantity : null;
  return {
    quotes: new Set(ls.map((l) => l.quote_id)).size,
    quantity,
    min: Math.min(...prices),
    max: Math.max(...prices),
    weighted,
    last: ls[ls.length - 1],
  };
});

// ---------- Courbe ----------
const W = 1000;
const H = 280;
const PAD = { left: 70, right: 40, top: 16, bottom: 34 };
const PALETTE = ["#e60005", "#2563eb", "#16a34a", "#d97706", "#7c3aed", "#0891b2"];

const time = (iso: string) => new Date(`${iso}T12:00:00`).getTime();

const chart = computed(() => {
  const ls = lines.value;
  if (!ls.length) return null;
  const threshold = history.value?.product?.threshold_price ?? null;
  const values = ls.map((l) => l.net_unit_price).concat(threshold != null ? [threshold] : []);
  let [y0, y1] = [Math.min(...values), Math.max(...values)];
  const pad = (y1 - y0) * 0.12 || Math.max(1, y1 * 0.1);
  [y0, y1] = [Math.max(0, y0 - pad), y1 + pad];
  let [t0, t1] = [time(ls[0].date), time(ls[ls.length - 1].date)];
  if (t0 === t1) [t0, t1] = [t0 - 15 * 864e5, t1 + 15 * 864e5];

  const x = (iso: string) => PAD.left + ((time(iso) - t0) / (t1 - t0)) * (W - PAD.left - PAD.right);
  const y = (v: number) => PAD.top + (1 - (v - y0) / (y1 - y0)) * (H - PAD.top - PAD.bottom);

  // Une série par client s'il y en a peu (couleurs + légende), sinon une seule.
  const keys = [...new Set(ls.map(clientKey))];
  const split = keys.length > 1 && keys.length <= PALETTE.length;
  const groups = split ? keys.map((k) => ls.filter((l) => clientKey(l) === k)) : [ls];
  const series = groups.map((g, i) => ({
    key: split ? clientKey(g[0]) : "all",
    label: split ? clientLabel(g[0]) : "",
    color: PALETTE[split ? i : 0],
    path: g.map((l, j) => `${j ? "L" : "M"}${x(l.date).toFixed(1)},${y(l.net_unit_price).toFixed(1)}`).join(" "),
    points: g.map((l) => ({
      x: x(l.date),
      y: y(l.net_unit_price),
      option: l.is_option,
      title: `${formatDate(l.date)} · ${l.number} · ${l.client_name}\n${formatNumber(l.quantity)} × ${formatUnitPrice(l.net_unit_price)} net`,
    })),
  }));

  const yTicks = Array.from({ length: 5 }, (_, i) => y0 + ((y1 - y0) * i) / 4).map((v) => ({ y: y(v), label: formatUnitPrice(v) }));
  const xTickCount = Math.min(6, Math.max(2, new Set(ls.map((l) => l.date)).size));
  const xTicks = Array.from({ length: xTickCount }, (_, i) => t0 + ((t1 - t0) * i) / (xTickCount - 1)).map((t) => ({
    x: PAD.left + ((t - t0) / (t1 - t0)) * (W - PAD.left - PAD.right),
    label: new Date(t).toLocaleDateString("fr-FR", { day: "2-digit", month: "2-digit", year: "2-digit" }),
  }));
  return {
    series,
    split,
    yTicks,
    xTicks,
    threshold: threshold != null ? { y: y(threshold), label: formatUnitPrice(threshold) } : null,
  };
});

const openQuote = (l: HistoryLine) => router.push(`/devis/${l.quote_id}`);
</script>

<template>
  <div class="page">
    <div class="page-header">
      <Button icon="pi pi-arrow-left" text rounded aria-label="Retour aux produits" @click="router.push('/produits')" />
      <div class="title">
        <h1>
          Évolution du chiffrage <span class="mono ref">{{ productRef }}</span>
        </h1>
        <span class="muted">
          <template v-if="history?.product">
            {{ history.product.designation }}
            <template v-if="history.product.enedis_code"> · ENEDIS {{ history.product.enedis_code }}</template>
            · prix public actuel {{ formatUnitPrice(history.product.public_price) }}
          </template>
          <template v-else-if="history">Référence absente du catalogue actuel</template>
        </span>
      </div>
    </div>

    <div class="card filters">
      <label for="history-clients">Clients</label>
      <MultiSelect
        v-model="selectedClients"
        input-id="history-clients"
        :options="clientOptions"
        option-label="label"
        option-value="value"
        display="chip"
        filter
        :max-selected-labels="4"
        placeholder="Tous les clients"
        class="clients-select"
        :disabled="!clientOptions.length"
      >
        <template #option="{ option }">
          <div class="client-opt">
            <span>{{ option.label }}</span>
            <span class="muted">{{ option.count }} ligne(s)</span>
          </div>
        </template>
      </MultiSelect>
      <Button
        v-if="selectedClients.length"
        label="Tous les clients"
        icon="pi pi-times"
        text
        size="small"
        severity="secondary"
        @click="selectedClients = []"
      />
      <span class="spacer" />
      <Checkbox v-model="withOptions" input-id="history-options" binary />
      <label for="history-options">Inclure les lignes en option</label>
    </div>

    <div v-if="!loading && !lines.length" class="card empty muted">
      {{
        history?.lines.length
          ? "Aucun devis pour ces critères."
          : "Cette référence n'a encore été chiffrée dans aucun devis."
      }}
    </div>

    <template v-else-if="kpis && chart">
      <div class="kpis">
        <div class="card kpi">
          <span class="kpi-label">Devis</span>
          <span class="kpi-value">{{ kpis.quotes }}</span>
        </div>
        <div class="card kpi">
          <span class="kpi-label">Quantité totale</span>
          <span class="kpi-value">{{ formatNumber(kpis.quantity) }}</span>
        </div>
        <div class="card kpi">
          <span class="kpi-label">Dernier prix net</span>
          <span class="kpi-value accent">{{ formatUnitPrice(kpis.last.net_unit_price) }}</span>
          <span class="kpi-sub muted">{{ formatDate(kpis.last.date) }} · {{ kpis.last.client_name }}</span>
        </div>
        <div class="card kpi">
          <span class="kpi-label">Prix net min – max</span>
          <span class="kpi-value">{{ formatUnitPrice(kpis.min) }} – {{ formatUnitPrice(kpis.max) }}</span>
          <span v-if="kpis.weighted != null" class="kpi-sub muted">
            moyenne pondérée {{ formatUnitPrice(kpis.weighted) }}
          </span>
        </div>
      </div>

      <section class="card">
        <h2>Prix unitaire net devisé</h2>
        <svg class="chart" :viewBox="`0 0 ${W} ${H}`" role="img" aria-label="Évolution du prix unitaire net">
          <g class="grid">
            <g v-for="t in chart.yTicks" :key="`y${t.y}`">
              <line :x1="PAD.left" :x2="W - PAD.right" :y1="t.y" :y2="t.y" />
              <text :x="PAD.left - 8" :y="t.y + 4" text-anchor="end">{{ t.label }}</text>
            </g>
            <text v-for="t in chart.xTicks" :key="`x${t.x}`" :x="t.x" :y="H - 10" text-anchor="middle">{{ t.label }}</text>
          </g>
          <g v-if="chart.threshold" class="threshold">
            <line :x1="PAD.left" :x2="W - PAD.right" :y1="chart.threshold.y" :y2="chart.threshold.y" />
            <text :x="W - PAD.right" :y="chart.threshold.y - 5" text-anchor="end">prix seuil {{ chart.threshold.label }}</text>
          </g>
          <g v-for="s in chart.series" :key="s.key">
            <path :d="s.path" :stroke="s.color" class="series" />
            <circle
              v-for="(p, i) in s.points"
              :key="i"
              :cx="p.x"
              :cy="p.y"
              r="5"
              :fill="p.option ? 'var(--app-surface)' : s.color"
              :stroke="s.color"
              class="point"
            >
              <title>{{ p.title }}</title>
            </circle>
          </g>
        </svg>
        <div v-if="chart.split" class="legend">
          <span v-for="s in chart.series" :key="s.key"><i :style="{ background: s.color }" />{{ s.label }}</span>
        </div>
        <p class="muted note">
          Prix après la remise de la ligne et la remise globale du devis. Survolez un point pour le détail<template
            v-if="withOptions"
            >, cercle vide : ligne en option</template
          >.
        </p>
      </section>

      <section class="card">
        <h2>Devis ({{ lines.length }} ligne{{ lines.length > 1 ? "s" : "" }})</h2>
        <DataTable
          :value="lines"
          size="small"
          striped-rows
          sort-field="date"
          :sort-order="-1"
          row-hover
          class="clickable"
          @row-click="openQuote($event.data)"
        >
          <Column field="date" header="Date" sortable style="width: 100px">
            <template #body="{ data }">{{ formatDate(data.date) }}</template>
          </Column>
          <Column field="number" header="Devis" sortable style="width: 140px">
            <template #body="{ data }"><span class="mono link">{{ data.number }}</span></template>
          </Column>
          <Column field="client_name" header="Client" sortable>
            <template #body="{ data }">
              {{ data.client_name }} <span class="muted mono small">{{ data.client_code || "ponctuel" }}</span>
              <Tag v-if="data.is_option" value="option" severity="secondary" class="opt-tag" />
            </template>
          </Column>
          <Column field="sales_rep" header="Commercial" sortable />
          <Column field="quantity" header="Qté" sortable class="num" style="width: 70px">
            <template #body="{ data }">{{ formatNumber(data.quantity) }}</template>
          </Column>
          <Column field="unit_price" header="PU HT" sortable class="num" style="width: 100px">
            <template #body="{ data }">{{ formatUnitPrice(data.unit_price) }}</template>
          </Column>
          <Column header="Remises" class="num" style="width: 110px">
            <template #body="{ data }">
              <span v-tooltip.top="'Remise de la ligne · remise globale du devis'" class="muted">
                {{ data.discount ? formatPct(data.discount) : "–" }} · {{ data.quote_discount ? formatPct(data.quote_discount) : "–" }}
              </span>
            </template>
          </Column>
          <Column field="net_unit_price" header="PU net" sortable class="num" style="width: 100px">
            <template #body="{ data }"><strong>{{ formatUnitPrice(data.net_unit_price) }}</strong></template>
          </Column>
          <Column field="public_price" header="Prix public" class="num" style="width: 100px">
            <template #body="{ data }"><span class="muted">{{ formatUnitPrice(data.public_price) }}</span></template>
          </Column>
          <Column field="lpn_price" header="LPN" class="num" style="width: 100px">
            <template #body="{ data }"><span class="muted">{{ formatUnitPrice(data.lpn_price) }}</span></template>
          </Column>
        </DataTable>
      </section>
    </template>
  </div>
</template>

<style scoped>
.title {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
}

.title h1 {
  margin: 0;
}

.ref {
  color: var(--app-accent);
}

.filters {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  flex-wrap: wrap;
}

.filters label {
  color: var(--app-muted);
}

.clients-select {
  min-width: 320px;
  max-width: 640px;
}

.client-opt {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  width: 100%;
}

.empty {
  padding: 2rem;
  text-align: center;
}

.kpis {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 1rem;
}

.kpi {
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
}

.kpi-label {
  color: var(--app-muted);
  font-size: 0.85rem;
}

.kpi-value {
  font-size: 1.45rem;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}

.kpi-value.accent {
  color: var(--app-accent);
}

.kpi-sub {
  font-size: 0.8rem;
}

h2 {
  margin: 0 0 0.75rem;
  font-size: 1rem;
}

.chart {
  display: block;
  width: 100%;
  height: auto;
}

.grid line {
  stroke: var(--app-border);
}

.grid text,
.threshold text {
  fill: var(--app-muted);
  font-size: 12px;
}

.threshold line {
  stroke: var(--app-warn);
  stroke-dasharray: 6 5;
}

.threshold text {
  fill: var(--app-warn);
}

.series {
  fill: none;
  stroke-width: 2;
}

.point {
  stroke-width: 2;
  cursor: default;
}

.legend {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem 1.25rem;
  font-size: 0.85rem;
}

.legend i {
  display: inline-block;
  width: 10px;
  height: 10px;
  margin-right: 0.4rem;
  border-radius: 50%;
}

.note {
  margin: 0.5rem 0 0;
  font-size: 0.8rem;
}

.clickable :deep(tbody tr) {
  cursor: pointer;
}

.link {
  color: var(--app-accent);
}

.small {
  font-size: 0.8em;
}

.opt-tag {
  margin-left: 0.4rem;
}
</style>
