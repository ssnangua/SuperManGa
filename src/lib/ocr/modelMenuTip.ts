import { i18n } from "../../i18n";

/** Hover tip for disabled model menu rows (detectors / OCR / neural). */
export function modelMenuDisabledTitle(options: {
  runtimeReady: boolean;
  modelInstalled: boolean;
}): string | undefined {
  const { runtimeReady, modelInstalled } = options;
  if (runtimeReady && modelInstalled) return undefined;
  if (!runtimeReady && !modelInstalled) {
    return i18n.global.t("modelNeedRuntimeAndInstall");
  }
  if (runtimeReady && !modelInstalled) {
    return i18n.global.t("modelNeedInstall");
  }
  return i18n.global.t("initRuntimeFirst");
}
