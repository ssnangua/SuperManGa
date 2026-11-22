<script setup lang="ts">
import first from "../assets/nav_first.svg?raw";
import prev from "../assets/nav_prev.svg?raw";
import next from "../assets/nav_next.svg?raw";
import last from "../assets/nav_last.svg?raw";
import play from "../assets/play.svg?raw";
import pause from "../assets/pause.svg?raw";
import bookmark from "../assets/bookmark.svg?raw";
import bookmarkOn from "../assets/bookmark_fill.svg?raw";
import panelOn from "../assets/left_sidebar_filled.svg?raw";
import full from "../assets/fullscreen_enter.svg?raw";
import exit from "../assets/fullscreen_exit.svg?raw";
import thumb from "../assets/thumb.svg?raw";
import modeScroll from "../assets/mode_infinite_scrolling.svg?raw";
import modeSingle from "../assets/mode_single_page.svg?raw";
import modeDouble from "../assets/mode_double_pages.svg?raw";
import dirLtr from "../assets/direction_left_to_right.svg?raw";
import dirRtl from "../assets/direction_right_to_left.svg?raw";
import fitCover from "../assets/fit_cover.svg?raw";
import fitContain from "../assets/fit_contain.svg?raw";
import fitOriginal from "../assets/fit_original.svg?raw";
import fitFixedWidth from "../assets/fit_fixed_width.svg?raw";
import fitOriginalScale from "../assets/fit_original_scale.svg?raw";
import zoomIn from "../assets/zoomin.svg?raw";
import zoomOut from "../assets/zoomout.svg?raw";
import close from "../assets/close.svg?raw";
import language from "../assets/language.svg?raw";
import expand from "../assets/expand.svg?raw";
import themeDark from "../assets/theme_dark.svg?raw";
import themeLight from "../assets/theme_light.svg?raw";
import minimalist from "../assets/minimalist_view_enter.svg?raw";
import minimalistExit from "../assets/minimalist_view_exit.svg?raw";
import image from "../assets/image.svg?raw";
import menu from "../assets/menu.svg?raw";
import setting from "../assets/setting.svg?raw";
import devtools from "../assets/devtools.svg?raw";
import checked from "../assets/checked.svg?raw";
import palette from "../assets/palette.svg?raw";
import translate from "../assets/translate.svg?raw";
import enhance from "../assets/enhance.svg?raw";
import loadingDots from "../assets/loading_dots_bounce.svg?raw";
import loadingDotsRotate from "../assets/loading_dots_rotate.svg?raw";
import download from "../assets/download.svg?raw";
import remove from "../assets/remove.svg?raw";
import clear from "../assets/clear.svg?raw";
import add from "../assets/add.svg?raw";
import regexp from "../assets/regexp.svg?raw";
import edit from "../assets/edit.svg?raw";
import move from "../assets/move.svg?raw";
import folderOpen from "../assets/folder_open.svg?raw";
import rotate from "../assets/rotate.svg?raw";
import keyboard from "../assets/keyboard.svg?raw";
import open from "../assets/open.svg?raw";
import save from "../assets/save.svg?raw";
import info from "../assets/info.svg?raw";
import help from "../assets/help.svg?raw";
import copy from "../assets/copy.svg?raw";
import eyeOpen from "../assets/eye_open.svg?raw";
import eyeClose from "../assets/eye_close.svg?raw";
import success from "../assets/success.svg?raw";
import fail from "../assets/fail.svg?raw";
import warning from "../assets/warning.svg?raw";
import unknow from "../assets/unknow.svg?raw";
import refresh from "../assets/refresh.svg?raw";
import github from "../assets/github.svg?raw";
import foldChevron from "../assets/fold_chevron.svg?raw";
import terminal from "../assets/terminal.svg?raw";
import lens from "../assets/lens.svg?raw";
import tools from "../assets/tools.svg?raw";
import newWindow from "../assets/new_window.svg?raw";

function clean(source: string) {
  return source
    .replace(/<\?xml[\s\S]*?\?>/gi, "")
    .replace(/<!DOCTYPE[\s\S]*?>/gi, "")
    .replace(/<svg\b([^>]*)>/i, (_, attrs: string) => `<svg${attrs.replace(/\s(?:width|height)=['"][^'"]*['"]/g, "")}>`);
}

const assets: Record<string, string> = {
  first: clean(first),
  prev: clean(prev),
  next: clean(next),
  last: clean(last),
  play: clean(play),
  pause: clean(pause),
  bookmark: clean(bookmark),
  bookmarkOn: clean(bookmarkOn),
  panelOn: clean(panelOn),
  full: clean(full),
  exit: clean(exit),
  thumb: clean(thumb),
  modeScroll: clean(modeScroll),
  modeSingle: clean(modeSingle),
  modeDouble: clean(modeDouble),
  dirLtr: clean(dirLtr),
  dirRtl: clean(dirRtl),
  fitCover: clean(fitCover),
  fitContain: clean(fitContain),
  fitOriginal: clean(fitOriginal),
  fitFixedWidth: clean(fitFixedWidth),
  fitOriginalScale: clean(fitOriginalScale),
  zoomIn: clean(zoomIn),
  zoomOut: clean(zoomOut),
  close: clean(close),
  language: clean(language),
  expand: clean(expand),
  themeDark: clean(themeDark),
  themeLight: clean(themeLight),
  minimalist: clean(minimalist),
  minimalistExit: clean(minimalistExit),
  image: clean(image),
  menu: clean(menu),
  setting: clean(setting),
  devtools: clean(devtools),
  checked: clean(checked),
  palette: clean(palette),
  translate: clean(translate),
  enhance: clean(enhance),
  loadingDots: clean(loadingDots),
  loadingDotsRotate: clean(loadingDotsRotate),
  download: clean(download),
  remove: clean(remove),
  clear: clean(clear),
  add: clean(add),
  regexp: clean(regexp),
  edit: clean(edit),
  move: clean(move),
  folderOpen: clean(folderOpen),
  rotate: clean(rotate),
  keyboard: clean(keyboard),
  open: clean(open),
  save: clean(save),
  info: clean(info),
  help: clean(help),
  copy: clean(copy),
  eyeOpen: clean(eyeOpen),
  eyeClose: clean(eyeClose),
  success: clean(success),
  fail: clean(fail),
  warning: clean(warning),
  unknow: clean(unknow),
  refresh: clean(refresh),
  github: clean(github),
  foldChevron: clean(foldChevron),
  terminal: clean(terminal),
  lens: clean(lens),
  tools: clean(tools),
  newWindow: clean(newWindow),
};

defineProps<{ name: string }>();
</script>

<template>
  <span v-if="assets[name]" class="asset-icon" v-html="assets[name]" />
  <span v-else class="asset-icon" aria-hidden="true">
    <svg viewBox="0 0 24 24">
      <path v-if="name === 'minus'" class="stroke" d="M5 12h14" />
      <path v-else-if="name === 'plus'" class="stroke" d="M12 5v14M5 12h14" />
      <path v-else class="stroke" d="M6 6l12 12M18 6L6 18" />
    </svg>
  </span>
</template>
