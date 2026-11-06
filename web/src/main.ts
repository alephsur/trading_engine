import { createApp } from "vue";
import { createPinia } from "pinia";
import { createVuetify } from "vuetify";
import {
  VApp,
  VBtn,
  VDialog,
  VCard,
  VCardTitle,
  VCardText,
  VCardActions,
  VTextField,
} from "vuetify/components";
const components = {
  VApp,
  VBtn,
  VDialog,
  VCard,
  VCardTitle,
  VCardText,
  VCardActions,
  VTextField,
};
import "vuetify/styles";
import "./style.css";
import App from "./App.vue";
createApp(App)
  .use(createPinia())
  .use(
    createVuetify({
      components,
      theme: {
        defaultTheme: "dark",
        themes: {
          dark: {
            colors: {
              background: "#0d1524",
              surface: "#141f31",
              primary: "#56debd",
              secondary: "#8ea4bf",
            },
          },
        },
      },
    }),
  )
  .mount("#app");
