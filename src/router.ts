import { createRouter, createWebHashHistory } from "vue-router";

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/devis" },
    { path: "/devis", component: () => import("./views/QuotesView.vue") },
    { path: "/devis/nouveau", component: () => import("./views/QuoteEditView.vue") },
    { path: "/devis/:id", component: () => import("./views/QuoteEditView.vue"), props: true },
    // Devis types : même éditeur, sans client ni prix.
    { path: "/devis-types/nouveau", component: () => import("./views/QuoteEditView.vue"), props: { template: true } },
    {
      path: "/devis-types/:id",
      component: () => import("./views/QuoteEditView.vue"),
      props: (r) => ({ id: r.params.id, template: true }),
    },
    { path: "/produits", component: () => import("./views/ProductsView.vue") },
    { path: "/produits/:productRef/chiffrage", component: () => import("./views/ProductHistoryView.vue"), props: true },
    { path: "/clients", component: () => import("./views/ClientsView.vue") },
    { path: "/listes", component: () => import("./views/PriceListsView.vue") },
    { path: "/statistiques", component: () => import("./views/StatsView.vue") },
    { path: "/documentation", component: () => import("./views/DocumentationView.vue") },
    { path: "/aide", component: () => import("./views/HelpView.vue") },
    { path: "/reglages", redirect: "/reglages/import" },
    { path: "/reglages/:section", component: () => import("./views/SettingsView.vue"), props: true },
  ],
});
