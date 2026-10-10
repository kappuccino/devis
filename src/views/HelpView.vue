<script setup lang="ts">
// Aide : les fonctionnalités de l'appli, par rubrique (contenu dans src/help.ts, tenu à jour).
import { computed, ref } from "vue";
import InputText from "primevue/inputtext";
import IconField from "primevue/iconfield";
import InputIcon from "primevue/inputicon";
import { HELP } from "../help";

const search = ref("");

/** Sans accents ni majuscules, pour la recherche. */
const plain = (s: string) => s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();

const sections = computed(() => {
  const q = plain(search.value.trim());
  if (!q) return HELP;
  return HELP.map((s) => ({
    ...s,
    items: s.items.filter((i) => plain(`${s.title} ${i.title} ${i.text} ${(i.keys ?? []).join(" ")}`).includes(q)),
  })).filter((s) => s.items.length);
});

function goTo(id: string) {
  document.getElementById(`aide-${id}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
}
</script>

<template>
  <div class="page">
    <div class="page-header">
      <h1>Aide</h1>
      <span class="muted">ce que fait l'appli, rubrique par rubrique</span>
      <span class="spacer" />
      <IconField>
        <InputIcon class="pi pi-search" />
        <InputText v-model="search" placeholder="Rechercher (ex. option, PDF, raccourci)…" style="width: 300px" />
      </IconField>
    </div>

    <nav v-if="!search.trim()" class="toc" aria-label="Rubriques">
      <button v-for="s in HELP" :key="s.id" type="button" class="toc-link" @click="goTo(s.id)">
        <i :class="s.icon" /> {{ s.title }}
      </button>
    </nav>

    <section v-for="s in sections" :id="`aide-${s.id}`" :key="s.id" class="card section">
      <h2><i :class="s.icon" /> {{ s.title }}</h2>
      <dl>
        <template v-for="item in s.items" :key="item.title">
          <dt>
            {{ item.title }}
            <span v-if="item.keys" class="keys">
              <kbd v-for="k in item.keys" :key="k">{{ k }}</kbd>
            </span>
          </dt>
          <dd>{{ item.text }}</dd>
        </template>
      </dl>
    </section>

    <div v-if="!sections.length" class="card muted empty">Aucune fonctionnalité ne correspond à « {{ search }} ».</div>
  </div>
</template>

<style scoped>
.toc {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}

.toc-link {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.35rem 0.75rem;
  border: 1px solid var(--app-border);
  border-radius: 999px;
  background: var(--app-surface);
  color: var(--app-text);
  font: inherit;
  font-size: 0.9rem;
  cursor: pointer;
}

.toc-link:hover {
  border-color: var(--app-accent);
  color: var(--app-accent);
}

.toc-link i {
  font-size: 0.85rem;
  color: var(--app-muted);
}

.section {
  scroll-margin-top: 1rem;
}

h2 {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin: 0 0 0.75rem;
  font-size: 1.05rem;
}

h2 i {
  color: var(--app-accent);
}

dl {
  display: grid;
  grid-template-columns: minmax(180px, 260px) 1fr;
  gap: 0.6rem 1.5rem;
  margin: 0;
}

dt {
  font-weight: 600;
}

dd {
  margin: 0;
  line-height: 1.45;
}

.keys {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-top: 4px;
}

kbd {
  padding: 1px 6px;
  border: 1px solid var(--app-border);
  border-bottom-width: 2px;
  border-radius: 4px;
  background: var(--app-bg);
  font-family: inherit;
  font-size: 0.75rem;
  font-weight: 500;
}

.empty {
  padding: 2rem;
  text-align: center;
}
</style>
