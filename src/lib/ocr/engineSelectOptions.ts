import { i18n, type MessageKey } from "../../i18n";
import type { AppSelectOption } from "../../components/AppCustomSelect.vue";
import { ocrEnginePlatformSupported } from "./availability";
import { ocrLangTagLabelKey } from "./langTags";
import {
  OCR_ENGINES,
  ocrEngineMeta,
  type OcrEngineId,
  type OcrEngineMeta,
} from "./engines";

const DROPDOWN_LANG_TAG_PREVIEW = 5;

export function ocrEngineSelectLabel(id: OcrEngineId): string {
  return i18n.global.t(ocrEngineMeta(id).nameKey as MessageKey);
}

/** Same capability blurb as settings / OCR models list. */
export function ocrEngineSelectNote(
  opt: OcrEngineMeta,
  missingPack: boolean,
): string {
  const hint = i18n.global.t(opt.hintKey as MessageKey);
  if (missingPack) return `${hint} ${i18n.global.t("ocrEngineDescNeedPack")}`;
  return hint;
}

/** Shared OCR-engine options for settings dropdown and title-bar submenu. */
export function buildOcrEngineSelectOptions(
  installedAssetIds: readonly string[],
  runtimeReady = true,
): AppSelectOption[] {
  const installed = new Set(installedAssetIds);
  return OCR_ENGINES.filter((opt) => ocrEnginePlatformSupported(opt.id))
    .map((opt) => {
      const missingPack = !!opt.assetId && !installed.has(opt.assetId);
      const needRuntime = !!opt.assetId && !runtimeReady;
      const preview = opt.tagLangs.slice(0, DROPDOWN_LANG_TAG_PREVIEW);
      return {
        id: opt.id,
        label: ocrEngineSelectLabel(opt.id),
        tags: [
          ...preview.map((code) => ({
            id: code,
            label: i18n.global.t(ocrLangTagLabelKey(code)),
          })),
          ...(opt.tagLangs.length > DROPDOWN_LANG_TAG_PREVIEW
            ? [{ id: "more", label: "…" }]
            : []),
        ],
        description: ocrEngineSelectNote(opt, missingPack),
        disabled: missingPack || needRuntime,
      };
    })
    .sort((a, b) => Number(a.disabled) - Number(b.disabled));
}
