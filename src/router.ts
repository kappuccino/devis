import { createRouter, createWebHashHistory } from "vue-router";

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/devis" },
    { path: "/devis", component: () => import("./views/QuotesView.vue") },
    { path: "/devis/nouveau", component: () => import("./views/QuoteEditView.vue") },
    { path: "/devis/:id", component: () => import("./views/QuoteEditView.vue"), props: true },
    { path: "/produits", component: () => import("./views/ProductsView.vue") },
    { path: "/clients", component: () => import("./views/ClientsView.vue") },
    { path: "/listes", component: () => import("./views/PriceListsView.vue") },
    { path: "/reglages", redirect: "/reglages/import" },
    { path: "/reglages/:section", component: () => import("./views/SettingsView.vue"), props: true },
  ],
});
