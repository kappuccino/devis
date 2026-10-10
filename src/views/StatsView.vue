<script setup lang="ts">
// Statistiques des devis : chiffres clés, devis par mois, meilleurs clients, commerciaux et
// produits, pour une année ou toute la base. Montants HT après remise globale, frais compris ;
// options exclues. Ce sont des montants devisés ; les affaires obtenues (coche de la liste des devis)
// sont comptées à part : taux de transformation et montant obtenu.
import { computed, onMounted, ref } from "vue";
import Select from "primevue/select";
import DataTable from "primevue/datatable";
import Column from "primevue/column";
import { useToast } from "primevue/usetoast";
import { api, type QuoteStats } from "../api";
import { errorMessage, formatEuro, formatNumber } from "../format";

const toast = useToast();
const stats = ref<QuoteStats | null>(null);
const loading = ref(true);
/** Année affichée ; null : toutes les années. */
const year = ref<string | null>(null);

const yearOptions = computed(() => [
  { label: "Toutes les années", value: null },
  ...(stats.value?.years ?? []).map((y) => ({ label: y, value: y })),
]);

async function load(y: string | null) {
  loading.value = true;
  try {
    stats.value = await api.quoteStats(y);
  } catch (e) {
    toast.add({ severity: "error", summary: "Statistiques", detail: errorMessage(e) });
  } finally {
    loading.value = false;
  }
}

/** Taux de transformation : part des devis devenus affaires obtenues. */
const rate = (won: number, count: number) => (count ? `${Math.round((won / count) * 100)} %` : "–");

const average = computed(() => (stats.value?.count ? stats.value.total / stats.value.count : 0));

/** Montant arrondi à l'euro (chiffres clés, histogramme). */
const euros = (v: number) => formatEuro(Math.round(v)).replace(/,00(?=\s?€)/, "");

// Devis par mois : les 12 mois de l'année choisie, ou du premier au dernier mois de la base
// (les mois sans devis apparaissent vides).
const monthLabel = new Intl.DateTimeFormat("fr-FR", { month: "short" });
const months = computed(() => {
  const data = stats.value?.by_month ?? [];
  const byKey = new Map(data.map((m) => [m.month, m]));
  let keys: string[];
  if (year.value) {
    keys = Array.from({ length: 12 }, (_, i) => `${year.value}-${String(i + 1).padStart(2, "0")}`);
  } else if (data.length) {
    keys = [];
    const [y0, m0] = data[0].month.split("-").map(Number);
    const [y1, m1] = data[data.length - 1].month.split("-").map(Number);
    for (let y = y0, m = m0; y < y1 || (y === y1 && m <= m1); ) {
      keys.push(`${y}-${String(m).padStart(2, "0")}`);
      if (++m > 12) {
        m = 1;
        y++;
      }
    }
  } else {
    keys = [];
  }
  const max = Math.max(1, ...keys.map((k) => byKey.get(k)?.total ?? 0));
  return keys.map((k) => {
    const m = byKey.get(k);
    const [y, mo] = k.split("-").map(Number);
    const name = monthLabel.format(new Date(y, mo - 1, 1)).replace(".", "");
    return {
      key: k,
      // Sur plusieurs années : l'année sous janvier (et sous le premier mois affiché).
      label: name,
      sub: !year.value && (mo === 1 || k === keys[0]) ? String(y) : "",
      count: m?.count ?? 0,
      total: m?.total ?? 0,
      wonCount: m?.won_count ?? 0,
      wonTotal: m?.won_total ?? 0,
      height: ((m?.total ?? 0) / max) * 100,
      // Part obtenue, en % de la barre.
      wonHeight: m?.total ? (m.won_total / m.total) * 100 : 0,
    };
  });
});

/** Part d'un montant dans le total (barres des classements). */
const share = (v: number) => (stats.value?.total ? Math.min(100, (v / stats.value.total) * 100) : 0);
const productMax = computed(() => Math.max(1, ...(stats.value?.top_products ?? []).map((p) => p.total)));

function selectYear(y: string | null) {
  year.value = y;
  load(y);
}

onMounted(async () => {
  // Par défaut : l'année en cours si elle a des devis, sinon toute la base.
  await load(null);
  const current = String(new Date().getFullYear());
  if (stats.value?.years.includes(current)) selectYear(current);
});
</script>

<template>
  <div class="page">
    <div class="page-header">
      <h1>Statistiques</h1>
      <span class="muted">montants HT devisés, remise globale déduite, frais compris, options exclues</span>
      <span class="spacer" />
      <Select
        :model-value="year"
        :options="yearOptions"
        option-label="label"
        option-value="value"
        aria-label="Année"
        class="year-select"
        @update:model-value="selectYear"
      />
    </div>

    <div v-if="stats && !stats.years.length && !loading" class="card empty muted">
      Aucun devis enregistré pour l'instant : les statistiques apparaîtront avec les premiers devis.
    </div>

    <template v-else-if="stats">
      <!-- Chiffres clés -->
      <div class="kpis">
        <div class="card kpi">
          <span class="kpi-label">Devis</span>
          <span class="kpi-value">{{ formatNumber(stats.count) }}</span>
        </div>
        <div class="card kpi">
          <span class="kpi-label">Montant total HT</span>
          <span class="kpi-value accent">{{ euros(stats.total) }}</span>
        </div>
        <div class="card kpi">
          <span class="kpi-label">Montant moyen</span>
          <span class="kpi-value">{{ euros(average) }}</span>
        </div>
        <div class="card kpi">
          <span class="kpi-label">Clients</span>
          <span class="kpi-value">{{ formatNumber(stats.clients) }}</span>
        </div>
        <div class="card kpi">
          <span class="kpi-label">Affaires obtenues</span>
          <span class="kpi-value">
            {{ formatNumber(stats.won_count) }}
            <span class="kpi-sub">{{ rate(stats.won_count, stats.count) }} des devis</span>
          </span>
        </div>
        <div class="card kpi">
          <span class="kpi-label">Montant obtenu HT</span>
          <span class="kpi-value won">
            {{ euros(stats.won_total) }}
            <span class="kpi-sub">{{ rate(stats.won_total, stats.total) }} du montant</span>
          </span>
        </div>
      </div>

      <!-- Devis par mois -->
      <section class="card">
        <div class="chart-head">
          <h2>Devis par mois</h2>
          <span class="legend"><i class="sw quoted" /> devisé <i class="sw won" /> dont obtenu</span>
        </div>
        <div class="chart" :class="{ dense: months.length > 18 }">
          <div
            v-for="m in months"
            :key="m.key"
            v-tooltip.top="
              m.count
                ? `${m.count} devis · ${euros(m.total)}` +
                  (m.wonCount ? ` · obtenus : ${m.wonCount} · ${euros(m.wonTotal)}` : '')
                : 'Aucun devis'
            "
            class="bar-col"
          >
            <span class="bar-value">{{ m.total ? euros(m.total) : "" }}</span>
            <div class="bar-track">
              <div class="bar" :style="{ height: `${m.height}%` }">
                <div class="bar-won" :style="{ height: `${m.wonHeight}%` }" />
              </div>
            </div>
            <span class="bar-label">{{ m.label }}</span>
            <span class="bar-count">{{ m.count ? `${m.count} devis` : "–" }}</span>
            <span class="bar-year">{{ m.sub }}</span>
          </div>
        </div>
      </section>

      <div class="two-cols">
        <!-- Meilleurs clients -->
        <section class="card">
          <h2>Meilleurs clients</h2>
          <DataTable :value="stats.top_clients" size="small" :loading="loading">
            <Column header="Client">
              <template #body="{ data }">
                <div class="name-cell">
                  <span>{{ data.label }}</span>
                  <span class="muted mono small">{{ data.code || "ponctuel" }}</span>
                </div>
                <div class="share"><div :style="{ width: `${share(data.total)}%` }" /></div>
              </template>
            </Column>
            <Column field="count" header="Devis" class="num" style="width: 70px" />
            <Column header="Obtenues" class="num" style="width: 90px">
              <template #body="{ data }">
                <span v-if="data.won_count" v-tooltip.top="euros(data.won_total)">{{ data.won_count }}</span>
                <span v-else class="muted">–</span>
              </template>
            </Column>
            <Column header="Montant HT" class="num" style="width: 130px">
              <template #body="{ data }">{{ euros(data.total) }}</template>
            </Column>
            <template #empty><span class="muted">Aucun devis.</span></template>
          </DataTable>
        </section>

        <!-- Par commercial -->
        <section class="card">
          <h2>Par commercial</h2>
          <DataTable :value="stats.by_sales_rep" size="small" :loading="loading">
            <Column header="Commercial">
              <template #body="{ data }">
                <span :class="{ muted: !data.label }">{{ data.label || "Non renseigné" }}</span>
                <div class="share"><div :style="{ width: `${share(data.total)}%` }" /></div>
              </template>
            </Column>
            <Column field="count" header="Devis" class="num" style="width: 70px" />
            <Column header="Obtenues" class="num" style="width: 90px">
              <template #body="{ data }">
                <span v-if="data.won_count" v-tooltip.top="euros(data.won_total)">{{ data.won_count }}</span>
                <span v-else class="muted">–</span>
              </template>
            </Column>
            <Column header="Montant HT" class="num" style="width: 130px">
              <template #body="{ data }">{{ euros(data.total) }}</template>
            </Column>
            <template #empty><span class="muted">Aucun devis.</span></template>
          </DataTable>
        </section>
      </div>

      <!-- Produits les plus chiffrés -->
      <section class="card">
        <h2>Produits les plus chiffrés</h2>
        <DataTable :value="stats.top_products" size="small" :loading="loading">
          <Column header="Référence" style="width: 130px">
            <template #body="{ data }"><span class="mono">{{ data.product_ref }}</span></template>
          </Column>
          <Column header="Désignation">
            <template #body="{ data }">
              <span>{{ data.designation }}</span>
              <div class="share"><div :style="{ width: `${(data.total / productMax) * 100}%` }" /></div>
            </template>
          </Column>
          <Column header="Quantité" class="num" style="width: 100px">
            <template #body="{ data }">{{ formatNumber(data.quantity) }}</template>
          </Column>
          <Column field="quotes" header="Devis" class="num" style="width: 70px" />
          <Column header="Montant HT" class="num" style="width: 130px">
            <template #body="{ data }">{{ euros(data.total) }}</template>
          </Column>
          <template #empty><span class="muted">Aucun produit.</span></template>
        </DataTable>
        <p class="muted note">Montant des lignes avant remise globale du devis.</p>
      </section>
    </template>
  </div>
</template>

<style scoped>
.year-select {
  min-width: 180px;
}

.empty {
  padding: 2rem;
  text-align: center;
}

.kpis {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
  gap: 1rem;
}

.kpi {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.kpi-label {
  color: var(--app-muted);
  font-size: 0.85rem;
}

.kpi-value {
  font-size: 1.6rem;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}

.kpi-sub {
  display: block;
  margin-top: 0.15rem;
  font-size: 0.75rem;
  font-weight: 400;
  color: var(--app-muted);
}

.kpi-value.won {
  color: var(--app-success);
}

.kpi-value.accent {
  color: var(--app-accent);
}

h2 {
  margin: 0 0 0.75rem;
  font-size: 1rem;
}

/* Histogramme : une colonne par mois, barre proportionnelle au montant. */
.chart {
  display: flex;
  align-items: stretch;
  gap: 6px;
  overflow-x: auto;
  padding-bottom: 0.25rem;
}

.bar-col {
  flex: 1 0 44px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
}

.chart.dense .bar-col {
  flex-basis: 34px;
}

.bar-value {
  min-height: 1.1em;
  font-size: 0.7rem;
  color: var(--app-muted);
  white-space: nowrap;
}

.bar-track {
  width: 100%;
  height: 160px;
  display: flex;
  align-items: flex-end;
  border-bottom: 1px solid var(--app-border);
}

.chart-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
}

.legend {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  font-size: 0.75rem;
  color: var(--app-muted);
}

.sw {
  display: inline-block;
  width: 10px;
  height: 10px;
  border-radius: 2px;
  margin-left: 0.5rem;
}

.sw.quoted {
  background: color-mix(in srgb, var(--app-accent) 75%, transparent);
}

.sw.won,
.bar-won {
  background: var(--app-success);
}

.bar {
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  overflow: hidden;
  width: 70%;
  margin: 0 auto;
  min-height: 0;
  border-radius: 4px 4px 0 0;
  background: color-mix(in srgb, var(--app-accent) 75%, transparent);
  transition: height 0.2s;
}

.bar-col:hover .bar {
  background: var(--app-accent);
}

.bar-label {
  font-size: 0.8rem;
  font-weight: 600;
}

.bar-count,
.bar-year {
  font-size: 0.7rem;
  color: var(--app-muted);
  white-space: nowrap;
}

.two-cols {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1rem;
}

.name-cell {
  display: flex;
  justify-content: space-between;
  gap: 0.75rem;
}

.small {
  font-size: 0.8em;
}

/* Barre fine sous le nom : part du montant total (ou du premier produit). */
.share {
  height: 3px;
  margin-top: 4px;
  border-radius: 2px;
  background: var(--app-border);
}

.share > div {
  height: 100%;
  border-radius: 2px;
  background: var(--app-accent);
}

.note {
  margin: 0.5rem 0 0;
  font-size: 0.8rem;
}
</style>
