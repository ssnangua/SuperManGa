<script setup lang="ts">
withDefaults(
  defineProps<{
    sub?: boolean;
    placement?: "down" | "up";
    role?: string;
  }>(),
  { placement: "down", role: "menu" },
);
</script>

<template>
  <div class="bar-menu" :class="{ sub, up: placement === 'up' }" :role="role">
    <slot />
  </div>
</template>

<style>
.bar-menu {
  position: absolute;
  top: 100%;
  left: 0;
  z-index: var(--z-menu);
  width: max-content;
  min-width: min(100px, calc(100vw - 8px));
  padding: 4px 0;
  overflow: visible;
  border-radius: 2px;
  border: 1px solid var(--line);
  background: var(--popup);
  box-shadow: var(--popup-shadow);
}

.bar-menu.sub {
  top: -5px;
  left: 100%;
  z-index: calc(var(--z-menu) + 1);
}

.bar-menu.sub::before {
  content: "";
  position: absolute;
  top: 0;
  bottom: 0;
  left: -8px;
  width: 8px;
}

.bar-menu.sub.flip {
  left: auto;
  right: 100%;
}

.bar-menu.sub.flip::before {
  left: auto;
  right: -8px;
}

.bar-menu.is-clamped:not(:has(.bar-menu)):not(.history-menu) {
  overflow-x: hidden;
  overflow-y: auto;
}

.bar-menu.history-menu {
  display: flex;
  flex-direction: column;
  padding-top: 0;
}

.bar-menu.history-menu .history-foot .sep {
  margin-top: 0;
}

.bar-menu.history-menu .history-list {
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  padding: 4px 0;
  overflow-x: hidden;
  overflow-y: auto;
}

.bar-menu.history-menu .history-foot {
  flex: none;
}

.has-sub {
  position: relative;
}

.bar-menu button {
  width: 100%;
  min-width: 0;
  max-width: 100%;
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 6px;
  padding: var(--control-pad-y) var(--control-pad-x);
  font-size: 13px;
  line-height: 18px;
  text-align: left;
  white-space: nowrap;
}

.bar-menu button > .label {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.bar-menu button > .label.menu-current {
  display: inline-flex;
  align-items: center;
}

.bar-menu .menu-current-prefix {
  font-weight: 600;
}

.bar-menu .lang-flag {
  --flag-w: 16px;
}

.bar-menu .menu-current .lang-flag,
.bar-menu .menu-current .menu-lang-icon {
  margin-right: 6px;
}

.bar-menu .menu-lang-icon,
.bar-menu .menu-lang-icon.asset-icon,
.bar-menu .menu-lang-icon.asset-icon svg {
  --icon-size: 14px;
  flex: none;
  width: 14px;
  height: 14px;
}

.bar-menu.is-clamped button {
  overflow: hidden;
  text-overflow: ellipsis;
}

.bar-menu button .key {
  flex: none;
  margin-left: auto;
  padding-left: 16px;
  color: var(--menu-key);
  font-size: 12px;
}

.bar-menu button .key .asset-icon,
.bar-menu button .key .asset-icon svg,
.bar-menu button > .menu-ico,
.bar-menu button > .mark,
.bar-menu button > .asset-icon,
.bar-menu button > .asset-icon svg,
.bar-menu button .mark .asset-icon,
.bar-menu button .mark .asset-icon svg {
  width: 14px;
  height: 14px;
}

.bar-menu button > .menu-ico,
.bar-menu button > .mark,
.bar-menu button > .asset-icon {
  flex: 0 0 14px;
}

.bar-menu button.choice {
  justify-content: flex-start;
}

.bar-menu.bar-menu-rich {
  /* Size to content; only cap width. A large min-width left empty space
     on short provider rows (OCR engine rows still grow via tags/copy). */
  width: max-content;
  max-width: min(440px, calc(100vw - 16px));
  padding: 4px;
}

.bar-menu button.choice-rich {
  align-items: flex-start;
  gap: 8px;
  padding: 8px 10px;
  white-space: normal;
  line-height: 1.4;
}

.bar-menu button.choice-rich > .mark {
  margin-top: 2px;
}

.bar-menu button.choice-rich.on .app-select-option-row-desc {
  color: color-mix(in srgb, var(--accent) 72%, transparent);
}

.bar-menu button .mark {
  color: var(--accent);
}

.bar-menu button:hover:not(:disabled),
.bar-menu button.is-hot:not(:disabled) {
  color: var(--chrome-active);
  background: var(--chrome-hover);
}

.bar-menu button {
  transition: none;
}

.menubar .bar-menu button.on,
.menubar .bar-menu button.on:hover,
.menubar .bar-menu button.on.is-hot {
  color: var(--accent);
  font-weight: 600;
}

.menubar .bar-menu button.on .key,
.menubar .bar-menu button.on:hover .key,
.menubar .bar-menu button.on.is-hot .key {
  font-weight: 400;
}

.bar-menu button:disabled {
  opacity: 0.38;
  cursor: default;
  background: transparent;
  color: inherit;
}

.bar-menu .sep {
  height: 1px;
  margin: 4px 8px;
  background: var(--line);
}
</style>
