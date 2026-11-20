/** Compare directory strings from settings vs the OS-resolved default. */
export function sameFsPath(a: string, b: string) {
  const norm = (value: string) =>
    value
      .trim()
      .replace(/[/\\]+$/, "")
      .replace(/\\/g, "/");
  const left = norm(a);
  const right = norm(b);
  if (left === right) return true;
  if (typeof navigator !== "undefined" && /windows/i.test(navigator.userAgent)) {
    return left.toLowerCase() === right.toLowerCase();
  }
  return false;
}
