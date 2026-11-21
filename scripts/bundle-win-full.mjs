/**
 * Build a Windows "full" portable zip: app + SuperManGaData (ORT / models).
 *
 * SuperManGa-{ver}-full-windows-x64.zip
 * └ SuperManGa-{ver}-full-windows-x64/
 *     ├ SuperManGa.exe
 *     ├ pdfium.dll
 *     └ SuperManGaData/data/…
 *
 * Prefers:
 *   - existing portable zip under src-tauri/target/release/bundle/portable/
 *     (falls back to packing from supermanga.exe; never runs tauri build)
 *   - local tools/pack-data/data/*.zip (+ manifest.json)
 *     (falls back to GitHub Release tag `data`)
 *
 * Usage:
 *   node scripts/bundle-win-full.mjs
 *   node scripts/bundle-win-full.mjs --force
 *   node scripts/bundle-win-full.mjs --skip-models
 *   node scripts/bundle-win-full.mjs --skip-runtime
 */
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  createReadStream,
  createWriteStream,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { pipeline } from "node:stream/promises";
import { Readable } from "node:stream";

const SAFE_NAME = "SuperManGa";
const RELEASE_BASE =
  "https://github.com/ssnangua/SuperManGa/releases/download/data";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const SEED_MANIFEST = join(
  root,
  "src-tauri",
  "resources",
  "data-manifest.json",
);
const LOCAL_PACK_DATA = join(root, "tools", "pack-data", "data");
const PORTABLE_DIR = join(
  root,
  "src-tauri",
  "target",
  "release",
  "bundle",
  "portable",
);
const RELEASE_DIR = join(root, "src-tauri", "target", "release");

function parseArgs(argv) {
  let force = false;
  let skipModels = false;
  let skipRuntime = false;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--force") force = true;
    else if (arg === "--skip-models") skipModels = true;
    else if (arg === "--skip-runtime") skipRuntime = true;
    else if (arg === "--help" || arg === "-h") {
      console.log(`Usage: node scripts/bundle-win-full.mjs [options]
  --force         Re-download / re-extract packs even if present
  --skip-models   Skip detector / OCR / neural packs
  --skip-runtime  Skip runtime-* packs`);
      process.exit(0);
    }
  }
  return { force, skipModels, skipRuntime };
}

function ensureDir(dir) {
  mkdirSync(dir, { recursive: true });
}

function appVersion() {
  const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  const tauri = JSON.parse(
    readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"),
  );
  return tauri.version || pkg.version;
}

function formatBytes(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function psPath(p) {
  return p.replace(/'/g, "''");
}

function extractZip(archive, dest) {
  ensureDir(dest);
  try {
    execFileSync("tar", ["-xf", archive, "-C", dest], {
      stdio: ["ignore", "inherit", "inherit"],
    });
    return;
  } catch {
    /* fall through */
  }
  if (process.platform === "win32") {
    execFileSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-Command",
        `Expand-Archive -LiteralPath '${psPath(archive)}' -DestinationPath '${psPath(dest)}' -Force`,
      ],
      { stdio: ["ignore", "inherit", "inherit"] },
    );
    return;
  }
  execFileSync("unzip", ["-o", archive, "-d", dest], {
    stdio: ["ignore", "inherit", "inherit"],
  });
}

function compressFolder(folderPath, zipPath) {
  const parent = dirname(folderPath);
  const name = folderPath.slice(parent.length + 1);
  const zipTmp = `${zipPath}.tmp.zip`;
  if (existsSync(zipTmp)) rmSync(zipTmp, { force: true });
  if (process.platform === "win32") {
    execFileSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-Command",
        `Compress-Archive -LiteralPath '${psPath(folderPath)}' -DestinationPath '${psPath(zipTmp)}' -Force`,
      ],
      { stdio: "inherit" },
    );
  } else {
    execFileSync("zip", ["-r", zipTmp, name], {
      cwd: parent,
      stdio: "inherit",
    });
  }
  if (existsSync(zipPath)) rmSync(zipPath, { force: true });
  try {
    renameSync(zipTmp, zipPath);
  } catch {
    copyFileSync(zipTmp, zipPath);
    rmSync(zipTmp, { force: true });
  }
}

async function downloadTo(url, dest, { force = false, label = "" } = {}) {
  if (existsSync(dest) && !force && statSync(dest).size > 0) {
    console.log(`  skip download (exists): ${label || dest}`);
    return;
  }
  ensureDir(dirname(dest));
  const tmp = `${dest}.partial`;
  if (existsSync(tmp)) rmSync(tmp, { force: true });
  console.log(`  downloading: ${label || url}`);
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
  const total = Number(res.headers.get("content-length")) || 0;
  let done = 0;
  let lastLog = 0;
  const body = Readable.fromWeb(res.body);
  body.on("data", (chunk) => {
    done += chunk.length;
    const now = Date.now();
    if (now - lastLog < 500 && done < total) return;
    lastLog = now;
    if (total > 0) {
      const pct = ((done / total) * 100).toFixed(1);
      process.stdout.write(
        `\r  progress: ${formatBytes(done)} / ${formatBytes(total)} (${pct}%)   `,
      );
    } else {
      process.stdout.write(`\r  progress: ${formatBytes(done)}   `);
    }
  });
  await pipeline(body, createWriteStream(tmp));
  process.stdout.write("\n");
  if (existsSync(dest)) rmSync(dest, { force: true });
  try {
    renameSync(tmp, dest);
  } catch {
    copyFileSync(tmp, dest);
    rmSync(tmp, { force: true });
  }
}

function loadManifest() {
  const local = join(LOCAL_PACK_DATA, "manifest.json");
  const path = existsSync(local) ? local : SEED_MANIFEST;
  console.log(`Manifest: ${path}`);
  const raw = readFileSync(path, "utf8");
  return JSON.parse(raw.replace(/^\uFEFF/, ""));
}

function packMeta(raw) {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const version = String(raw.version || "").trim();
  if (!version) return null;
  const bytes =
    Number.isFinite(raw.bytes) && raw.bytes > 0 ? Number(raw.bytes) : 0;
  const sha256 = String(raw.sha256 || "")
    .trim()
    .toLowerCase();
  return { version, bytes, sha256 };
}

function packEntries(manifest, { skipRuntime, skipModels }) {
  const out = [];
  const push = (section, key, raw) => {
    const meta = packMeta(raw);
    if (!meta) return;
    const stem = `${section}-${key}`;
    out.push({
      stem,
      version: meta.version,
      folder: `${stem}-v${meta.version}`,
      bytes: meta.bytes,
      sha256: meta.sha256,
    });
  };
  if (!skipRuntime) {
    for (const [key, raw] of Object.entries(manifest.runtime || {})) {
      push("runtime", key, raw);
    }
  }
  if (!skipModels) {
    for (const section of ["detector", "ocr", "neural"]) {
      for (const [key, raw] of Object.entries(manifest[section] || {})) {
        push(section, key, raw);
      }
    }
  }
  return out;
}

async function sha256File(path) {
  const hash = createHash("sha256");
  await pipeline(createReadStream(path), hash);
  return hash.digest("hex");
}

async function verifyArchive(archivePath, { bytes, sha256 }) {
  const size = statSync(archivePath).size;
  if (bytes && size !== bytes) {
    throw new Error(
      `size mismatch for ${archivePath}: expected ${bytes}, got ${size}`,
    );
  }
  if (sha256) {
    const digest = await sha256File(archivePath);
    if (digest.toLowerCase() !== sha256.toLowerCase()) {
      throw new Error(
        `checksum mismatch for ${archivePath}: expected ${sha256}, got ${digest}`,
      );
    }
  }
}

async function resolveArchive(entry, cacheDir, force) {
  const archiveName = `${entry.folder}.zip`;
  const localPath = join(LOCAL_PACK_DATA, archiveName);
  if (existsSync(localPath) && statSync(localPath).size > 0) {
    console.log(`  use local pack: tools/pack-data/data/${archiveName}`);
    return localPath;
  }
  const archivePath = join(cacheDir, archiveName);
  const url = `${RELEASE_BASE}/${archiveName}`;
  await downloadTo(url, archivePath, { force, label: archiveName });
  return archivePath;
}

async function installPack(dataDir, cacheDir, entry, force) {
  const { stem, version, folder, bytes, sha256 } = entry;
  const dest = join(dataDir, folder);
  const marker = join(dest, ".ok");
  if (existsSync(dest) && !force) {
    console.log(`  skip extract (exists): ${folder}`);
    return;
  }
  console.log(`\n[${stem}] v${version}`);
  const archivePath = await resolveArchive(entry, cacheDir, force);
  if (bytes || sha256) {
    await verifyArchive(archivePath, { bytes, sha256 });
    console.log(`  verified${sha256 ? " sha256" : ""}${bytes ? " size" : ""}`);
  }
  if (existsSync(dest)) rmSync(dest, { recursive: true, force: true });
  const extractTo = join(cacheDir, `extract-${folder}`);
  if (existsSync(extractTo)) rmSync(extractTo, { recursive: true, force: true });
  ensureDir(extractTo);
  console.log(`  extracting…`);
  extractZip(archivePath, extractTo);
  const kids = readdirSync(extractTo);
  let source = extractTo;
  if (kids.length === 1 && statSync(join(extractTo, kids[0])).isDirectory()) {
    source = join(extractTo, kids[0]);
  }
  ensureDir(dataDir);
  try {
    renameSync(source, dest);
  } catch {
    execFileSync(
      process.platform === "win32" ? "xcopy" : "cp",
      process.platform === "win32"
        ? [source, dest, "/E", "/I", "/Y"]
        : ["-a", source, dest],
      { stdio: "inherit" },
    );
  }
  rmSync(extractTo, { recursive: true, force: true });
  writeFileSync(marker, `${stem}@${version}\n`);
  console.log(`  installed → data/${folder}/`);
}

function ensurePortableZip(version) {
  ensureDir(PORTABLE_DIR);
  const zipPath = join(PORTABLE_DIR, `${SAFE_NAME}-${version}-windows-x64.zip`);
  if (existsSync(zipPath) && statSync(zipPath).size > 0) {
    console.log(`Portable zip (reuse): ${zipPath}`);
    return zipPath;
  }
  const exePath = join(RELEASE_DIR, "supermanga.exe");
  if (!existsSync(exePath)) {
    console.error(
      `Missing portable zip and ${exePath}.\nRun: npm run build:win`,
    );
    process.exit(1);
  }
  console.log(
    "Portable zip missing; packing from existing release exe (no tauri rebuild)…",
  );
  execFileSync(process.execPath, [join(root, "scripts", "pack-portable.mjs")], {
    cwd: root,
    stdio: "inherit",
  });
  if (!existsSync(zipPath) || statSync(zipPath).size < 1) {
    console.error(`pack-portable.mjs did not produce ${zipPath}`);
    process.exit(1);
  }
  return zipPath;
}

function copyTree(src, dest) {
  ensureDir(dest);
  for (const name of readdirSync(src)) {
    const from = join(src, name);
    const to = join(dest, name);
    if (statSync(from).isDirectory()) copyTree(from, to);
    else copyFileSync(from, to);
  }
}

function flattenExtractedPortable(extractRoot, packageDir) {
  const kids = readdirSync(extractRoot);
  let source = extractRoot;
  if (kids.length === 1 && statSync(join(extractRoot, kids[0])).isDirectory()) {
    source = join(extractRoot, kids[0]);
  }
  for (const name of readdirSync(source)) {
    const from = join(source, name);
    const to = join(packageDir, name);
    if (statSync(from).isDirectory()) copyTree(from, to);
    else copyFileSync(from, to);
  }
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  const version = appVersion();
  const thinStem = `${SAFE_NAME}-${version}-windows-x64`;
  const fullStem = `${SAFE_NAME}-${version}-full-windows-x64`;
  const portableZip = ensurePortableZip(version);

  const stagingRoot = mkdtempSync(join(tmpdir(), "jmr-full-"));
  const packageDir = join(stagingRoot, fullStem);
  const dataExtractRoot = join(stagingRoot, "data-staging");
  const cacheDir = join(stagingRoot, "cache");
  ensureDir(packageDir);
  ensureDir(cacheDir);

  console.log(`\nExtracting portable → ${fullStem}/`);
  const portableExtract = join(stagingRoot, "portable-extract");
  extractZip(portableZip, portableExtract);
  flattenExtractedPortable(portableExtract, packageDir);

  const dataRoot = join(packageDir, "SuperManGaData");
  const dataDir = join(dataRoot, "data");
  ensureDir(dataDir);

  const manifest = loadManifest();
  writeFileSync(
    join(dataDir, "manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );
  const entries = packEntries(manifest, opts);
  console.log(
    `\nData packs (${entries.length}): ${entries.map((e) => e.folder).join(", ") || "(none)"}`,
  );
  for (const entry of entries) {
    await installPack(dataDir, cacheDir, entry, opts.force);
  }

  // Keep unused staging dir name reserved for clarity in logs.
  void dataExtractRoot;

  ensureDir(PORTABLE_DIR);
  const outZip = join(PORTABLE_DIR, `${fullStem}.zip`);
  console.log(`\nZipping ${fullStem}.zip …`);
  compressFolder(packageDir, outZip);
  rmSync(stagingRoot, { recursive: true, force: true });

  console.log(`\nFull portable: ${outZip}`);
  console.log(`(thin portable unchanged: ${thinStem}.zip)`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
