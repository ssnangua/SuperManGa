import { i18n } from "../../i18n";
import { translationProviderLabel } from "./providerSelectOptions";
import type { TranslationProviderId } from "./types";
import { TRANSLATION_PROVIDER_IDS } from "./types";

function providerName(id: string): string {
  if ((TRANSLATION_PROVIDER_IDS as readonly string[]).includes(id)) {
    return translationProviderLabel(id as TranslationProviderId);
  }
  if (id === "list_models") return i18n.global.t("aiPullModels");
  return id;
}

function unwrapInvokeError(raw: string): string {
  let s = raw.trim();
  const m = s.match(/^(?:Error|error):\s*([\s\S]+)$/);
  if (m?.[1]) s = m[1].trim();
  return s;
}

/** Map stable Rust `TRANSLATE_*` codes to the current UI locale. */
export function formatTranslationError(raw: unknown): string {
  const s = unwrapInvokeError(String(raw ?? ""));
  if (!s) return i18n.global.t("trErrGeneric");

  switch (s) {
    case "TRANSLATE_NEED_URL":
      return i18n.global.t("trErrNeedUrl");
    case "TRANSLATE_NEED_AI":
      return i18n.global.t("trErrNeedAi");
    case "TRANSLATE_NEED_BAIDU":
      return i18n.global.t("trErrNeedBaidu");
    case "TRANSLATE_NEED_DEEPL":
      return i18n.global.t("trErrNeedDeepl");
    case "TRANSLATE_NEED_CAIYUN_TOKEN":
      return i18n.global.t("trErrNeedCaiyunToken");
    case "TRANSLATE_NEED_YOUDAO":
      return i18n.global.t("trErrNeedYoudao");
    case "TRANSLATE_NEED_TENCENT":
      return i18n.global.t("trErrNeedTencent");
    case "TRANSLATE_NEED_VOLC":
      return i18n.global.t("trErrNeedVolc");
    case "TRANSLATE_NEED_ALIYUN":
      return i18n.global.t("trErrNeedAliyun");
    case "TRANSLATE_TENCENT_NOT_OPENED":
      return i18n.global.t("trErrTencentNotOpened");
    case "TRANSLATE_EMPTY_INPUT":
      return i18n.global.t("trErrEmptyInput");
    case "TRANSLATE_CAIYUN_COUNT":
      return i18n.global.t("trErrCaiyunCount");
    default:
      break;
  }

  if (s.startsWith("TRANSLATE_YOUDAO_API:")) {
    const code = s.slice("TRANSLATE_YOUDAO_API:".length);
    return i18n.global.t("trErrYoudaoApi", { code });
  }
  if (s.startsWith("TRANSLATE_TENCENT_API:")) {
    const msg = s.slice("TRANSLATE_TENCENT_API:".length);
    return i18n.global.t("trErrTencentApi", { msg });
  }
  if (s.startsWith("TRANSLATE_VOLC_API:")) {
    const msg = s.slice("TRANSLATE_VOLC_API:".length);
    return i18n.global.t("trErrVolcApi", { msg });
  }
  if (s.startsWith("TRANSLATE_ALIYUN_API:")) {
    const rest = s.slice("TRANSLATE_ALIYUN_API:".length);
    const pipe = rest.indexOf("|");
    const code = pipe >= 0 ? rest.slice(0, pipe) : rest;
    const msg = pipe >= 0 ? rest.slice(pipe + 1) : "";
    return i18n.global.t("trErrAliyunApi", { code, msg });
  }

  if (s.startsWith("TRANSLATE_EMPTY:")) {
    const provider = s.slice("TRANSLATE_EMPTY:".length);
    return i18n.global.t("trErrEmpty", { provider: providerName(provider) });
  }

  if (s.startsWith("TRANSLATE_PROVIDER:")) {
    const id = s.slice("TRANSLATE_PROVIDER:".length);
    return i18n.global.t("trErrProvider", { id: providerName(id) });
  }

  if (s.startsWith("TRANSLATE_CAIYUN_TARGET:")) {
    const target = s.slice("TRANSLATE_CAIYUN_TARGET:".length);
    return i18n.global.t("trErrCaiyunTarget", { target });
  }

  if (s.startsWith("TRANSLATE_BAIDU_API:")) {
    const rest = s.slice("TRANSLATE_BAIDU_API:".length);
    const pipe = rest.indexOf("|");
    const code = pipe >= 0 ? rest.slice(0, pipe) : rest;
    const msg = pipe >= 0 ? rest.slice(pipe + 1) : "";
    return i18n.global.t("trErrBaiduApi", { code, msg });
  }

  if (s.startsWith("TRANSLATE_HTTP:")) {
    const rest = s.slice("TRANSLATE_HTTP:".length);
    const pipe = rest.indexOf("|");
    const head = pipe >= 0 ? rest.slice(0, pipe) : rest;
    const detail = pipe >= 0 ? rest.slice(pipe + 1) : "";
    const colon = head.indexOf(":");
    const provider = colon >= 0 ? head.slice(0, colon) : head;
    const status = colon >= 0 ? head.slice(colon + 1) : "";
    const name = providerName(provider);
    if (detail) {
      return i18n.global.t("trErrHttpDetail", {
        provider: name,
        status,
        detail,
      });
    }
    return i18n.global.t("trErrHttp", { provider: name, status });
  }

  // Unknown / transport errors (ureq etc.) — keep raw text.
  return s;
}

export function isTranslationErrorCode(raw: unknown): boolean {
  const s = unwrapInvokeError(String(raw ?? ""));
  return s.startsWith("TRANSLATE_");
}
