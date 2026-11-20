/**
 * Fetch platform pdfium native library into src-tauri/resources/pdfium/.
 * Keep PDFIUM_VERSION in sync with src-tauri/src/archive/pdf.rs.
 *
 * Usage:
 *   node scripts/fetch-pdfium.mjs
 *   node scripts/fetch-pdfium.mjs --target x86_64-apple-darwin
 *   node scripts/fetch-pdfium.mjs --force
 *   node scripts/fetch-pdfium.mjs --optional   # warn and exit 0 on failure
 */
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

/** Keep in sync with `PDFIUM_VERSION` in src-tauri/src/archive/pdf.rs */
const PDFIUM_VERSION = "chromium/8152";

const ASSETS = {
  "x86_64-pc-windows-msvc": {
    archive: "pdfium-win-x64.tgz",
    library: "pdfium.dll",
  },
  "i686-pc-windows-msvc": {
    archive: "pdfium-win-x86.tgz",
    library: "pdfium.dll",
  },
  "aarch64-pc-windows-msvc": {
    archive: "pdfium-win-arm64.tgz",
    library: "pdfium.dll",
  },
  "x86_64-apple-darwin": {
    archive: "pdfium-mac-x64.tgz",
    library: "libpdfium.dylib",
  },
  "aarch64-apple-darwin": {
    archive: "pdfium-mac-arm64.tgz",
    library: "libpdfium.dylib",
  },
  "x86_64-unknown-linux-gnu": {
    archive: "pdfium-linux-x64.tgz",
    library: "libpdfium.so",
  },
  "aarch64-unknown-linux-gnu": {
    archive: "pdfium-linux-arm64.tgz",
    library: "libpdfium.so",
  },
};

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "src-tauri/resources/pdfium");

function parseArgs(argv) {
  let target = "";
  let force = false;
  let optional = false;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--force") force = true;
    else if (arg === "--optional") optional = true;
    else if (arg === "--target") target = argv[++i] || "";
    else if (arg.startsWith("--target=")) target = arg.slice("--target=".length);
  }
  return { target, force, optional };
}

function hostRustTarget() {
  const platform = process.platform;
  const arch = process.arch;
  if (platform === "win32") {
    if (arch === "x64") return "x86_64-pc-windows-msvc";
    if (arch === "ia32") return "i686-pc-windows-msvc";
    if (arch === "arm64") return "aarch64-pc-windows-msvc";
  }
  if (platform === "darwin") {
    if (arch === "x64") return "x86_64-apple-darwin";
    if (arch === "arm64") return "aarch64-apple-darwin";
  }
  if (platform === "linux") {
    if (arch === "x64") return "x86_64-unknown-linux-gnu";
    if (arch === "arm64") return "aarch64-unknown-linux-gnu";
  }
  throw new Error(`Unsupported host platform: ${platform}/${arch}`);
}

function hasProxyEnv() {
  return Boolean(
    process.env.HTTPS_PROXY ||
      process.env.HTTP_PROXY ||
      process.env.https_proxy ||
      process.env.http_proxy,
  );
}

function mirrorUrls(archive) {
  const encoded = PDFIUM_VERSION.replace("/", "%2F");
  const official = `https://github.com/bblanchon/pdfium-binaries/releases/download/${encoded}/${archive}`;
  const mirrors = [
    `https://ghfast.top/${official}`,
    `https://gh-proxy.com/${official}`,
    `https://mirror.ghproxy.com/${official}`,
    `https://ghproxy.net/${official}`,
    `https://gh.llkk.cc/${official}`,
    `https://github.moeyy.xyz/${official}`,
  ];
  // With a local VPN/proxy, hit GitHub first; otherwise try mirrors before official.
  return hasProxyEnv() ? [official, ...mirrors] : [...mirrors, official];
}

function downloadTo(url, dest) {
  const args = [
    "-fL",
    "--retry",
    "2",
    "--connect-timeout",
    "20",
    "--max-time",
    "180",
    "-o",
    dest,
    url,
  ];
  // curl on Windows may ignore process env for proxies; pass explicitly when set.
  const proxy =
    process.env.HTTPS_PROXY ||
    process.env.HTTP_PROXY ||
    process.env.https_proxy ||
    process.env.http_proxy;
  if (proxy) args.unshift("-x", proxy);
  execFileSync("curl", args, {
    stdio: ["ignore", "inherit", "inherit"],
    env: process.env,
  });
}

function findLibrary(dir, library) {
  const direct = join(dir, library);
  if (existsSync(direct) && statSync(direct).isFile()) return direct;
  for (const nested of ["bin", "lib"]) {
    const path = join(dir, nested, library);
    if (existsSync(path) && statSync(path).isFile()) return path;
  }
  for (const name of readdirSync(dir, { withFileTypes: true })) {
    if (!name.isDirectory()) continue;
    const found = findLibrary(join(dir, name.name), library);
    if (found) return found;
  }
  return null;
}

function clearOutputLibs() {
  mkdirSync(outDir, { recursive: true });
  for (const name of readdirSync(outDir)) {
    if (name === "README.md" || name === ".gitkeep") continue;
    rmSync(join(outDir, name), { recursive: true, force: true });
  }
}

function resolveTarget(targetArg) {
  return (
    targetArg ||
    process.env.PDFIUM_TARGET ||
    process.env.TAURI_ENV_TARGET_TRIPLE ||
    hostRustTarget()
  );
}

function run() {
  const { target: targetArg, force } = parseArgs(process.argv.slice(2));
  const target = resolveTarget(targetArg);
  const asset = ASSETS[target];
  if (!asset) {
    throw new Error(
      `Unknown target "${target}". Supported: ${Object.keys(ASSETS).join(", ")}`,
    );
  }

  const destLib = join(outDir, asset.library);
  const stampPath = join(outDir, ".pdfium-stamp");
  const stamp = `${PDFIUM_VERSION}\n${target}\n${asset.library}\n`;
  if (!force && existsSync(destLib) && existsSync(stampPath)) {
    const prev = readFileSync(stampPath, "utf8");
    if (prev === stamp && statSync(destLib).size > 1_000_000) {
      console.log(`pdfium already present: ${destLib}`);
      return;
    }
  }

  clearOutputLibs();
  const work = mkdtempSync(join(tmpdir(), "jmr-pdfium-"));
  const archivePath = join(work, asset.archive);
  let lastErr = "no mirror tried";
  let ok = false;
  for (const url of mirrorUrls(asset.archive)) {
    try {
      console.log(`Downloading ${url}`);
      downloadTo(url, archivePath);
      const size = statSync(archivePath).size;
      if (size < 1_000_000) {
        lastErr = `download too small from ${url} (${size} bytes)`;
        continue;
      }
      ok = true;
      break;
    } catch (err) {
      lastErr = `${url}: ${err instanceof Error ? err.message : String(err)}`;
    }
  }
  if (!ok) {
    rmSync(work, { recursive: true, force: true });
    throw new Error(`Failed to download pdfium: ${lastErr}`);
  }

  execFileSync("tar", ["-xzf", archivePath, "-C", work], { stdio: "inherit" });
  const found = findLibrary(work, asset.library);
  if (!found) {
    rmSync(work, { recursive: true, force: true });
    throw new Error(`Extracted archive but ${asset.library} was not found`);
  }
  mkdirSync(outDir, { recursive: true });
  copyFileSync(found, destLib);
  writeFileSync(stampPath, stamp);
  rmSync(work, { recursive: true, force: true });
  console.log(`Installed ${destLib} (${statSync(destLib).size} bytes)`);
}

try {
  run();
} catch (err) {
  const optional = process.argv.includes("--optional");
  const message = err instanceof Error ? err.message : String(err);
  if (optional) {
    console.warn(`[fetch-pdfium] ${message}`);
    console.warn(
      "[fetch-pdfium] Continuing without pdfium. Encrypted / vector PDFs need: npm run fetch:pdfium",
    );
    process.exit(0);
  }
  console.error(message);
  process.exit(1);
}
