import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
import { useSettingsStore } from "./stores/settings";
import "./styles/tokens.css";

const app = createApp(App);
const pinia = createPinia();
app.use(pinia);

if ("__TAURI_INTERNALS__" in window) {
  await useSettingsStore().load();
}

app.mount("#app");
