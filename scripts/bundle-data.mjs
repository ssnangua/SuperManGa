/**
 * Build a cross-platform data pack: every ORT runtime and every model.
 *
 * SuperManGaData.zip
 * └── SuperManGaData
 *     └── data
 *         ├── manifest.json
 *         ├── runtime-windows-x64-cuda13-v1.30.0
 *         ├── runtime-macos-arm64-cpu-v1.30.0
 *         ├── runtime-linux-arm64-cpu-v1.30.0
 *         ├── detector-ctd-v1.0.0
 *         ├── ocr-manga-ocr-v1.0.0
 *         ├── neural-manga-janai-2x-v1.0.0
 *         └── …
 *
 * Unpack next to the exe on Windows, or into the OS app-data directory on
 * macOS / Linux, so `SuperManGaData/data/` lands on the app's data root.
 * Runtimes the current OS does not use can be deleted by hand.
 *
 * Prefers local tools/pack-data/data/*.zip (+ manifest.json).
 * Missing packs fall back to GitHub Release tag `data`.
 *
 * Usage:
 *   node scripts/bundle-data.mjs
 *   node scripts/bundle-data.mjs --force
 *   node scripts/bundle-data.mjs --skip-models
 *   node scripts/bundle-data.mjs --skip-runtime
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

const DATA_NAME = "SuperManGaData";
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
      console.log(`Usage: node scripts/bundle-data.mjs [options]
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

function formatBytes(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
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
  if (existsSync(zipPath)) rmSync(zipPath, { force: true });
  // bsdtar writes ZIP64. Compress-Archive cannot store an archive this large.
  try {
    execFileSync("tar", ["-a", "-cf", zipPath, "-C", parent, name], {
      stdio: "inherit",
    });
    return;
  } catch (err) {
    if (process.platform === "win32") throw err;
  }
  execFileSync("zip", ["-r", zipPath, name], {
    cwd: parent,
    stdio: "inherit",
  });
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
  console.log(`  installed → data/${folder}/`);
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  const stagingRoot = mkdtempSync(join(tmpdir(), "smg-data-"));
  const dataRoot = join(stagingRoot, DATA_NAME);
  const dataDir = join(dataRoot, "data");
  const cacheDir = join(stagingRoot, "cache");
  ensureDir(dataDir);
  ensureDir(cacheDir);

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

  ensureDir(LOCAL_PACK_DATA);
  const outZip = join(LOCAL_PACK_DATA, `${DATA_NAME}.zip`);
  console.log(`\nZipping ${DATA_NAME}.zip …`);
  compressFolder(dataRoot, outZip);
  rmSync(stagingRoot, { recursive: true, force: true });

  const size = statSync(outZip).size;
  console.log(`\nData pack: ${outZip}`);
  console.log(`Size: ${formatBytes(size)}`);
  console.log("Publish this zip on the cloud drive, not as a GitHub Release asset.");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
