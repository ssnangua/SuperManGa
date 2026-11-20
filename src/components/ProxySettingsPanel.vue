<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { computed, ref, watch } from "vue";

import { testDownloadProxy } from "../lib/render/assets";
import { notifyError, notifyWarning } from "../lib/appToast";
import { inTauri, downloadNetworkConfig, settings } from "../reader";
import AppCustomSelect, { type AppSelectOption } from "./AppCustomSelect.vue";
import SuggestInput, { type SuggestInputItem } from "./SuggestInput.vue";
import Icon from "./Icon.vue";
import SwitchToggle from "./SwitchToggle.vue";

const { t } = useI18n();
const props = defineProps<{ active: boolean }>();

type TestPhase = "idle" | "pending" | "ok" | "fail";

const DEFAULT_PROXY_TEST_URL = "https://github.com";
const proxyTypeOptions: readonly AppSelectOption[] = [
  { id: "http", label: "HTTP" },
  { id: "socks5", label: "SOCKS5" },
  { id: "socks4", label: "SOCKS4" },
];

const proxyTestSuggestions: readonly SuggestInputItem[] = [
  {
    id: "https://github.com",
    label: "GitHub",
    description: "https://github.com",
  },
  {
    id: "https://www.google.com",
    label: "Google",
    description: "https://www.google.com",
  },
];

const proxyTestUrl = ref(DEFAULT_PROXY_TEST_URL);
const showPassword = ref(false);
const testPhase = ref<TestPhase>("idle");
const testError = ref("");
const lastResolvedFingerprint = ref<string | null>(null);

const proxyTestFingerprint = computed(() =>
  [
    settings.proxyEnabled ? "1" : "0",
    settings.proxyType,
    settings.proxyHost.trim(),
    String(settings.proxyPort || ""),
    settings.proxyUser.trim(),
    settings.proxyPass,
    proxyTestUrl.value.trim(),
  ].join("\0"),
);

const testIcon = computed(() => {
  switch (testPhase.value) {
    case "pending":
      return "refresh";
    case "ok":
      return "success";
    case "fail":
      return "fail";
    default:
      return "unknow";
  }
});

watch(
  () => props.active,
  (active) => {
    if (!active) return;
    testPhase.value = "idle";
    testError.value = "";
    lastResolvedFingerprint.value = null;
    showPassword.value = false;
  },
);

watch(proxyTestFingerprint, (fp) => {
  if (testPhase.value === "pending") return;
  if (lastResolvedFingerprint.value === null) return;
  if (fp !== lastResolvedFingerprint.value) {
    testPhase.value = "idle";
    testError.value = "";
    lastResolvedFingerprint.value = null;
  }
});

function onProxyType(id: string) {
  settings.proxyType = id === "socks5" || id === "socks4" ? id : "http";
}

function onPortInput(ev: Event) {
  const raw = (ev.target as HTMLInputElement).value;
  const n = Number(raw);
  settings.proxyPort = Number.isFinite(n) && n > 0 ? Math.min(65535, Math.round(n)) : 0;
}

async function runProxyTest() {
  if (testPhase.value === "pending") return;
  if (!inTauri()) {
    notifyError(t("desktopOnly"));
    return;
  }
  const url = proxyTestUrl.value.trim();
  if (!url) {
    notifyWarning(t("proxyTestNeedUrl"));
    return;
  }
  if (settings.proxyEnabled) {
    if (!settings.proxyHost.trim() || !settings.proxyPort) {
      notifyWarning(t("proxyTestNeedHost"));
      return;
    }
  }

  const fpWhenStarted = proxyTestFingerprint.value;
  testPhase.value = "pending";
  testError.value = "";
  try {
    await testDownloadProxy(downloadNetworkConfig(), url);
    if (proxyTestFingerprint.value !== fpWhenStarted) {
      testPhase.value = "idle";
      lastResolvedFingerprint.value = null;
      return;
    }
    testPhase.value = "ok";
    lastResolvedFingerprint.value = fpWhenStarted;
  } catch (error) {
    if (proxyTestFingerprint.value !== fpWhenStarted) {
      testPhase.value = "idle";
      lastResolvedFingerprint.value = null;
      return;
    }
    testError.value = String(error);
    testPhase.value = "fail";
    lastResolvedFingerprint.value = fpWhenStarted;
    notifyError(testError.value);
  }
}
</script>

<template>
  <div class="app-tab-pane proxy-settings-panel" role="tabpanel">
    <div class="proxy-body">
      <div class="asset-row proxy-section-head">
        <h3 class="settings-section-title">{{ t("proxyEnable") }}</h3>
        <div class="asset-actions">
          <SwitchToggle v-model="settings.proxyEnabled" :aria-label="t('proxyEnable')" />
        </div>
      </div>

      <template v-if="settings.proxyEnabled">
        <div class="proxy-row">
          <div class="proxy-row-main proxy-row-main--baseline">
            <span class="proxy-label proxy-label--short">{{ t("proxyType") }}</span>
            <div class="proxy-field">
              <AppCustomSelect
                class="proxy-select"
                :model-value="settings.proxyType"
                :options="proxyTypeOptions"
                :ariaLabel="t('proxyType')"
                @update:model-value="onProxyType"
              />
            </div>
          </div>
        </div>

        <div class="proxy-row">
          <div class="proxy-row-main proxy-row-main--baseline">
            <span class="proxy-label proxy-label--short">{{ t("proxyHost") }}</span>
            <input
              v-model="settings.proxyHost"
              class="proxy-text-input"
              type="text"
              placeholder="127.0.0.1"
              autocomplete="off"
              :aria-label="t('proxyHost')"
            />
          </div>
        </div>

        <div class="proxy-row">
          <div class="proxy-row-main proxy-row-main--baseline">
            <span class="proxy-label proxy-label--short">{{ t("proxyPort") }}</span>
            <div class="proxy-field">
              <input
                class="proxy-text-input proxy-text-input--port"
                type="number"
                inputmode="numeric"
                min="1"
                max="65535"
                step="1"
                :value="settings.proxyPort || ''"
                placeholder="7890"
                autocomplete="off"
                :aria-label="t('proxyPort')"
                @input="onPortInput"
              />
            </div>
          </div>
        </div>

        <div class="proxy-row">
          <div class="proxy-row-main proxy-row-main--baseline">
            <span class="proxy-label proxy-label--short">{{ t("proxyUser") }}</span>
            <input
              v-model="settings.proxyUser"
              class="proxy-text-input"
              type="text"
              :placeholder="t('optional')"
              autocomplete="off"
              :aria-label="t('proxyUser')"
            />
          </div>
        </div>

        <div class="proxy-row">
          <div class="proxy-row-main proxy-row-main--baseline">
            <span class="proxy-label proxy-label--short">{{ t("proxyPass") }}</span>
            <div class="proxy-field">
              <div class="input-button-group">
                <input
                  v-model="settings.proxyPass"
                  :type="showPassword ? 'text' : 'password'"
                  :placeholder="t('optional')"
                  autocomplete="new-password"
                  :aria-label="t('proxyPass')"
                />
                <button
                  type="button"
                  class="input-group-btn"
                  :title="showPassword ? t('hidePassword') : t('showPassword')"
                  :aria-label="showPassword ? t('hidePassword') : t('showPassword')"
                  @click="showPassword = !showPassword"
                >
                  <Icon :name="showPassword ? 'eyeOpen' : 'eyeClose'" />
                </button>
              </div>
            </div>
          </div>
        </div>
      </template>

      <div class="proxy-row">
        <div class="proxy-row-main proxy-row-main--baseline">
          <span class="proxy-label proxy-label--short">{{ t("proxyTestLabel") }}</span>
          <div class="proxy-field">
            <div class="proxy-inline-row">
              <SuggestInput
                v-model="proxyTestUrl"
                class="proxy-test-suggest"
                :items="proxyTestSuggestions"
                placeholder="https://github.com"
                :ariaLabel="t('proxyTestLabel')"
              />
              <button
                type="button"
                class="connection-test-btn"
                :class="{
                  success: testPhase === 'ok',
                  danger: testPhase === 'fail',
                }"
                :disabled="testPhase === 'pending'"
                :title="testError || undefined"
                @click="runProxyTest"
              >
                <Icon :name="testIcon" :class="{ spinning: testPhase === 'pending' }" />
                {{ t("proxyTest") }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>

  </div>
</template>

<style scoped>
/* Proxy settings row layout. */
.proxy-settings-panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.proxy-body {
  display: flex;
  flex-direction: column;
  gap: 20px;
  padding: 16px;
  background: var(--bg);
  border-radius: 8px;
}

.proxy-section-head {
  align-items: center;
}

.proxy-section-head .settings-section-title {
  flex: 1 1 auto;
  min-width: 0;
}

.proxy-body--mirror {
  gap: 12px;
}

.proxy-body--mirror .settings-hint {
  margin: 0;
}

.proxy-hf-mirror {
  display: block;
  width: 100%;
  min-width: 0;
}

.proxy-hf-mirror :deep(.suggest-input-field) {
  width: 100%;
}

.proxy-row {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.proxy-row-main {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  min-width: 0;
}

.proxy-row-main--baseline {
  align-items: baseline;
}

.proxy-label {
  font-size: 14px;
  color: var(--ink);
  white-space: nowrap;
  flex: 1 1 60%;
  min-width: 0;
}

/* Short label ~30%; must NOT keep growing or it steals field width */
.proxy-label--short {
  flex: 0 0 30%;
  width: 30%;
  min-width: 30%;
  max-width: 30%;
}

/* Fill remaining width in row. */
.proxy-field {
  flex: 1 1 65%;
  min-width: 0;
  max-width: 100%;
}

.proxy-field .input-button-group {
  width: 100%;
  min-width: 0;
}

.proxy-text-input {
  box-sizing: border-box;
  flex: 1 1 65%;
  min-width: 0;
  max-width: 100%;
  width: auto;
}

.proxy-text-input--port {
  width: 120px;
  max-width: 100%;
  flex: 0 0 120px;
  /* Override global textfield hide —show steppers only on hover/focus */
  appearance: auto;
}

.proxy-text-input--port::-webkit-outer-spin-button,
.proxy-text-input--port::-webkit-inner-spin-button {
  appearance: auto;
  margin: 0;
  opacity: 0;
}

.proxy-text-input--port:hover::-webkit-outer-spin-button,
.proxy-text-input--port:hover::-webkit-inner-spin-button,
.proxy-text-input--port:focus::-webkit-outer-spin-button,
.proxy-text-input--port:focus::-webkit-inner-spin-button,
.proxy-text-input--port:focus-visible::-webkit-outer-spin-button,
.proxy-text-input--port:focus-visible::-webkit-inner-spin-button {
  opacity: 1;
}

.proxy-select {
  display: block;
  width: 100%;
  min-width: 0;
}

.proxy-inline-row {
  display: flex;
  align-items: stretch;
  gap: 8px;
  min-width: 0;
  width: 100%;
}

.proxy-stretch-input {
  box-sizing: border-box;
  flex: 1 1 auto;
  min-width: 0;
  width: auto;
}

.proxy-test-suggest {
  flex: 1 1 auto;
  min-width: 0;
  max-width: none;
  width: auto;
}

.proxy-test-suggest :deep(.suggest-input-field) {
  width: 100%;
}

.proxy-inline-row :deep(.connection-test-btn),
.proxy-inline-row .icon-only {
  flex: 0 0 auto;
}
</style>
