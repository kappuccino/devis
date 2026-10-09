import { createApp } from "vue";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import ConfirmationService from "primevue/confirmationservice";
import Tooltip from "primevue/tooltip";
import Aura from "@primeuix/themes/aura";
import { definePreset } from "@primeuix/themes";
import "primeicons/primeicons.css";
import "./styles.css";
import App from "./App.vue";
import { router } from "./router";
import { applyTheme } from "./theme";

applyTheme();

// Couleur principale : le rouge du logo Cahors (#E60005).
const Cahors = definePreset(Aura, {
  semantic: {
    primary: {
      50: "#fff0f0",
      100: "#ffdddd",
      200: "#ffc0c1",
      300: "#ff9496",
      400: "#f8333a",
      500: "#e60005",
      600: "#c20004",
      700: "#a00307",
      800: "#84090c",
      900: "#6e0d0f",
      950: "#3d0203",
    },
  },
});

createApp(App)
  .use(router)
  .use(PrimeVue, { theme: { preset: Cahors, options: { darkModeSelector: ".app-dark" } }, locale: { emptyMessage: "Aucun résultat" } })
  .use(ToastService)
  .use(ConfirmationService)
  .directive("tooltip", Tooltip)
  .mount("#app");
