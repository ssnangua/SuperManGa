/** Title bar / toolbar popup menus that should dismiss without acting on the canvas. */
export function isAppMenuOpen(): boolean {
  if (document.querySelector(".menubar .bar-menu")) return true;
  for (const menu of document.querySelectorAll<HTMLElement>(".toolbar .bar-menu")) {
    if (getComputedStyle(menu).display !== "none") return true;
  }
  return false;
}
