import { i18n, type MessageKey } from "../../i18n";
import type { AppSelectOption } from "../../components/AppCustomSelect.vue";
import {
  TEXT_DETECTORS,
  textDetectorMeta,
  type TextDetectorId,
  type TextDetectorMeta,
} from "./detectors";

export function textDetectorSelectLabel(id: TextDetectorId): string {
  return i18n.global.t(textDetectorMeta(id).nameKey as MessageKey);
}

export function textDetectorSelectNote(opt: TextDetectorMeta, missingPack: boolean): string {
  const hint = i18n.global.t(opt.hintKey as MessageKey);
  if (missingPack) return `${hint} ${i18n.global.t("ocrEngineDescNeedPack")}`;
  return hint;
}

/** Shared detector options for settings dropdown and title-bar submenu. */
export function buildTextDetectorSelectOptions(
  installedAssetIds: readonly string[],
): AppSelectOption[] {
  const installed = new Set(installedAssetIds);
  return TEXT_DETECTORS.map((opt) => {
    const missingPack = !!opt.assetId && !installed.has(opt.assetId);
    return {
      id: opt.id,
      label: textDetectorSelectLabel(opt.id),
      description: textDetectorSelectNote(opt, missingPack),
      disabled: missingPack,
    };
  }).sort((a, b) => {
    const byAvail = Number(a.disabled) - Number(b.disabled);
    if (byAvail !== 0) return byAvail;
    if (a.id === "off") return -1;
    if (b.id === "off") return 1;
    if (a.id === "dbnet") return -1;
    if (b.id === "dbnet") return 1;
    return 0;
  });
}
