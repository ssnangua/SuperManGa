let demoUrls: string[] | null = null;

export function setDemoUrls(urls: string[] | null) {
  demoUrls = urls;
}

export function pageUrl(origin: string, sessionId: number, index: number): string {
  if (demoUrls) return demoUrls[index] ?? "";
  return `${origin}/${sessionId}/${index}`;
}
