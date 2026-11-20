import { createApp } from "vue";
import LogApp from "./LogApp.vue";
import { i18n } from "./i18n";

createApp(LogApp).use(i18n).mount("#app");
