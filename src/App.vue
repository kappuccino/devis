<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { indexing, startDocs } from "./docs/store";
import UpdateDialog from "./components/UpdateDialog.vue";
import { checkForUpdate, updatesSupported } from "./updater";
import Toast from "primevue/toast";
import ConfirmDialog from "primevue/confirmdialog";
import { useRoute } from "vue-router";
import logo from "./assets/logo-cahors.png";
import { MOD } from "./format";

const route = useRoute();

const nav = [
  { to: "/devis", label: "Devis", icon: "pi pi-file-edit" },
  { to: "/produits", label: "Produits", icon: "pi pi-box" },
  { to: "/clients", label: "Clients", icon: "pi pi-users" },
  { to: "/listes", label: "Listes de prix", icon: "pi pi-list" },
  { to: "/statistiques", label: "Statistiques", icon: "pi pi-chart-bar" },
  // Documentation technique (ex-PDF Finder) : à part, un peu plus bas.
  { to: "/documentation", label: "Documentation", icon: "pi pi-book", separated: true },
];

// Barre latérale repliable (icônes seules), état mémorisé d'une session à l'autre.
const STORAGE_KEY = "sidebar-collapsed";
const collapsed = ref(false);
try {
  collapsed.value = localStorage.getItem(STORAGE_KEY) === "1";
} catch {}
watch(collapsed, (v) => {
  try {
    localStorage.setItem(STORAGE_KEY, v ? "1" : "0");
  } catch {}
});

const toggle = () => (collapsed.value = !collapsed.value);
const tip = (label: string) => (collapsed.value ? label : null);
// /devis/12 garde « Devis » actif ; /devis/nouveau a sa propre entrée.
const isActive = (to: string) =>
  route.path === to || (route.path.startsWith(`${to}/`) && route.path !== "/devis/nouveau");

function onKeydown(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "b") {
    e.preventDefault();
    toggle();
  }
}
onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  // Index de la documentation mis à jour en tâche de fond, sans bloquer l'appli.
  startDocs();
  // Nouvelle version ? Vérifié discrètement quelques secondes après le lancement
  // (application installée sous Windows ; la version Mac est construite en local, sans mise à jour).
  if (import.meta.env.PROD && updatesSupported) setTimeout(() => checkForUpdate(true), 5000);
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKeydown));
</script>

<template>
  <div class="layout">
    <nav class="sidebar" :class="{ collapsed }">
      <div class="top">
        <span class="brand label">
          <img :src="logo" alt="Cahors" />
          <span class="brand-app">Devis</span>
        </span>
        <button
          v-tooltip.right="`${collapsed ? 'Déplier' : 'Replier'} (${MOD}B)`"
          class="icon-btn"
          :aria-label="collapsed ? 'Déplier la barre latérale' : 'Replier la barre latérale'"
          @click="toggle"
        >
          <!-- Icône « panneau latéral » (comme sur claude.ai) : flèche vers la gauche pour replier,
               vers la droite pour déplier. -->
          <svg class="panel-icon" viewBox="0 0 24 24" aria-hidden="true">
            <rect x="3" y="3" width="18" height="18" rx="2.5" />
            <path d="M9 3v18" />
            <path v-if="collapsed" d="m14 9 3 3-3 3" />
            <path v-else d="m16 15-3-3 3-3" />
          </svg>
        </button>
      </div>

      <RouterLink
        v-tooltip.right="tip('Nouveau devis')"
        to="/devis/nouveau"
        class="nav-link new-quote"
        :class="{ active: route.path === '/devis/nouveau' }"
      >
        <i class="pi pi-plus" />
        <span class="label">Nouveau devis</span>
      </RouterLink>

      <RouterLink
        v-for="item in nav"
        :key="item.to"
        v-tooltip.right="tip(item.label)"
        :to="item.to"
        class="nav-link"
        :class="{ active: isActive(item.to), separated: item.separated }"
      >
        <i :class="item.icon" />
        <span class="label">{{ item.label }}</span>
        <i
          v-if="item.to === '/documentation' && indexing"
          v-tooltip.right="'Indexation de la documentation en cours'"
          class="pi pi-spin pi-spinner busy"
        />
      </RouterLink>

      <span class="spacer" />

      <RouterLink
        v-tooltip.right="tip('Réglages')"
        to="/reglages"
        class="nav-link"
        :class="{ active: isActive('/reglages') }"
      >
        <i class="pi pi-cog" />
        <span class="label">Réglages</span>
      </RouterLink>
    </nav>
    <main class="content">
      <RouterView />
    </main>
    <Toast position="bottom-right" />
    <ConfirmDialog />
    <UpdateDialog />
  </div>
</template>

<style scoped>
.layout {
  display: flex;
  height: 100vh;
  /* Largeur de la barre latérale, pour les vues en plein écran qui la laissent visible. */
  --sidebar-width: 220px;
}

.layout:has(.sidebar.collapsed) {
  --sidebar-width: 58px;
}

/* Barre latérale sombre : le logo Cahors (texte blanc) y est lisible. */
.sidebar {
  flex: none;
  width: 220px;
  background: var(--app-sidebar-bg);
  color: var(--app-sidebar-text);
  padding: 0.75rem 0.6rem;
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow: hidden;
  transition: width 0.18s ease;
}

.panel-icon {
  width: 18px;
  height: 18px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.6;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.sidebar.collapsed {
  width: 58px;
}

.top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 40px;
  margin-bottom: 0.75rem;
}

.brand {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding-left: 0.6rem;
}

.brand img {
  width: 120px;
  height: auto;
  display: block;
}

.brand-app {
  font-size: 0.72rem;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  color: var(--app-sidebar-muted);
}

.icon-btn {
  flex: none;
  width: 36px;
  height: 36px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--app-sidebar-muted);
  cursor: pointer;
}

.icon-btn:hover {
  background: var(--app-sidebar-hover);
  color: #fff;
}

.label {
  white-space: nowrap;
  transition: opacity 0.12s ease;
}

.collapsed .label {
  opacity: 0;
  pointer-events: none;
}

/* Replié, seul le bouton de dépliage reste en haut. */
.collapsed .brand {
  display: none;
}

.nav-link {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  height: 36px;
  padding: 0 0.7rem;
  border-radius: 8px;
  color: var(--app-sidebar-text);
  text-decoration: none;
}

.nav-link i {
  flex: none;
  width: 16px;
  text-align: center;
}

.nav-link:hover {
  background: var(--app-sidebar-hover);
  color: #fff;
}

.nav-link.active {
  background: color-mix(in srgb, var(--app-accent) 22%, transparent);
  color: #fff;
  font-weight: 600;
  box-shadow: inset 3px 0 0 var(--app-accent);
}

.nav-link.separated {
  margin-top: 1rem;
}

/* Indexation de la documentation en cours. */
.nav-link .busy {
  margin-left: auto;
  font-size: 0.8rem;
  color: var(--app-sidebar-muted);
}

.collapsed .nav-link .busy {
  display: none;
}

.new-quote {
  margin-bottom: 0.6rem;
  font-weight: 600;
}

.new-quote i {
  color: #fff;
  background: var(--app-accent);
  border-radius: 50%;
  width: 20px;
  height: 20px;
  line-height: 20px;
  font-size: 0.7rem;
  margin-left: -2px;
}

.spacer {
  flex: 1;
}

.content {
  flex: 1;
  min-width: 0;
  overflow: auto;
}
</style>
