import { createApp } from "vue";
import App from "./App.vue";
import { installConsoleBridge } from "./lib/appLog";
import { installEscapeBlurTextFieldListener } from "./lib/escapeBlurTextField";
import { selfCheck } from "./lib/selfCheck";
import { i18n } from "./i18n";
import "./styles.css";

installConsoleBridge();
installEscapeBlurTextFieldListener();
selfCheck();
createApp(App).use(i18n).mount("#app");

