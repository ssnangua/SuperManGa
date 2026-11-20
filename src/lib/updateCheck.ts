import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { reactive } from "vue";
import { i18n } from "../i18n";
import { inTauri, downloadNetworkConfig, settings } from "../reader";

export const GITHUB_REPO_URL = "https://github.com/ssnangua/SuperManGa";
export const GITHUB_RELEASES_LATEST_URL = `${GITHUB_REPO_URL}/releases/latest`;

export type UpdateCheckResult = {
  updateAvailable: boolean;
  currentVersion: string;
  latestVersion: string;
  htmlUrl: string;
};

export type UpdateUiPhase = "idle" | "checking" | "available" | "info";

export const updateUi = reactive({
  phase: "idle" as UpdateUiPhase,
  latestVersion: "",
  htmlUrl: GITHUB_RELEASES_LATEST_URL,
  infoTitle: "",
  infoMessage: "",
  /** Optional technical detail (e.g. HTTP error); shown behind a disclosure. */
  infoDetail: "",
});

export function dismissUpdateUi() {
  updateUi.phase = "idle";
}

export async function openGithubRepo() {
  if (!inTauri()) {
    window.open(GITHUB_REPO_URL, "_blank", "noopener,noreferrer");
    return;
  }
  await openUrl(GITHUB_REPO_URL);
}

export async function openUpdateDownloadPage(url?: string) {
  const target = (url || updateUi.htmlUrl || GITHUB_RELEASES_LATEST_URL).trim();
  if (!target) return;
  if (!inTauri()) {
    window.open(target, "_blank", "noopener,noreferrer");
    return;
  }
  await openUrl(target);
}

function showAvailable(result: UpdateCheckResult) {
  updateUi.latestVersion = result.latestVersion;
  updateUi.htmlUrl = result.htmlUrl || GITHUB_RELEASES_LATEST_URL;
  updateUi.phase = "available";
}

function showInfo(title: string, message: string, detail = "") {
  updateUi.infoTitle = title;
  updateUi.infoMessage = message;
  updateUi.infoDetail = detail.trim();
  updateUi.phase = "info";
}

async function fetchUpdateCheck(): Promise<UpdateCheckResult> {
  return invoke<UpdateCheckResult>("check_app_update", {
    network: downloadNetworkConfig(),
  });
}

/** Pull a readable reason from Tauri invoke / network errors. */
function invokeErrorText(err: unknown): string {
  if (typeof err === "string") return err.trim();
  if (err instanceof Error) return err.message.trim();
  if (err && typeof err === "object") {
    const bag = err as Record<string, unknown>;
    for (const key of ["message", "error", "data"] as const) {
      const value = bag[key];
      if (typeof value === "string" && value.trim()) return value.trim();
    }
  }
  const text = String(err ?? "").trim();
  return text && text !== "[object Object]" ? text : "";
}

/** Silent startup check: only opens the download prompt when a newer release exists. */
export async function checkUpdateOnStartup(): Promise<void> {
  if (!inTauri() || !settings.checkUpdateOnStartup) return;
  // Same gate as auto-resume: only the first live reader process probes GitHub.
  const isPrimary = await invoke<boolean>("is_primary_instance");
  if (!isPrimary) return;
  // Defer so first paint / boot IPC are not competing with a GitHub probe.
  await new Promise<void>((resolve) => {
    window.setTimeout(resolve, 1500);
  });
  try {
    const result = await fetchUpdateCheck();
    if (result.updateAvailable) showAvailable(result);
  } catch {
    // Network / no releases yet — stay quiet on startup.
  }
}

/** Manual check from the Help menu: show checking UI immediately, then the outcome. */
export async function checkForUpdates(): Promise<void> {
  if (updateUi.phase === "checking") return;
  if (!inTauri()) {
    showInfo(i18n.global.t("checkForUpdates"), i18n.global.t("desktopOnly"));
    return;
  }
  updateUi.phase = "checking";
  try {
    const result = await fetchUpdateCheck();
    if (result.updateAvailable) showAvailable(result);
    else showInfo(i18n.global.t("checkForUpdates"), i18n.global.t("updateUpToDate"));
  } catch (err) {
    showInfo(
      i18n.global.t("updateCheckFailed"),
      i18n.global.t("updateCheckFailedHint"),
      invokeErrorText(err),
    );
  }
}
