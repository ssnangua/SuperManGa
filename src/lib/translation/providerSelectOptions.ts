import { i18n, type MessageKey } from "../../i18n";
import type { AppSelectOption } from "../../components/AppCustomSelect.vue";
import { TRANSLATION_PROVIDER_IDS, type TranslationProviderId } from "./types";

export function translationProviderLabel(id: TranslationProviderId): string {
  return i18n.global.t(`provider_${id}` as MessageKey);
}

/** Shared translation-provider options for settings dropdown and title-bar submenu. */
export function buildTranslationProviderSelectOptions(): AppSelectOption[] {
  return TRANSLATION_PROVIDER_IDS.map((id) => ({
    id,
    label: translationProviderLabel(id),
    description: i18n.global.t(`providerDesc_${id}` as MessageKey),
  }));
}
