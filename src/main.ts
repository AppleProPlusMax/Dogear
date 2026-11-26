import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
import { useSettingsStore } from "./stores/settings";
import "./styles/tokens.css";

// 禁用非输入区域的浏览器默认右键菜单（返回/刷新/另存为/打印），输入框保留剪切/复制/粘贴菜单。
window.addEventListener("contextmenu", (event) => {
  const target = event.target;
  if (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  ) {
    return;
  }
  event.preventDefault();
});

// 屏蔽桌面应用不需要的浏览器快捷键（打印/保存网页/前进后退/正式版刷新）。
window.addEventListener("keydown", (event) => {
  const key = event.key.toLowerCase();
  if (event.altKey && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
    event.preventDefault();
    return;
  }
  if ((event.ctrlKey || event.metaKey) && (key === "p" || key === "s")) {
    event.preventDefault();
    return;
  }
  if (!import.meta.env.DEV && (event.key === "F5" || ((event.ctrlKey || event.metaKey) && key === "r"))) {
    event.preventDefault();
  }
});

const app = createApp(App);
const pinia = createPinia();
app.use(pinia);
app.mount("#app");

if ("__TAURI_INTERNALS__" in window) {
  void useSettingsStore().load();
}
