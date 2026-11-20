/**
 * Pack Windows release as a zip for GitHub Releases.
 *
 * SuperManGa-{ver}-windows-x64.zip
 * └ SuperManGa-{ver}-windows-x64/
 *     ├ SuperManGa.exe
 *     └ pdfium.dll
 *
 * Names stay ASCII without spaces.
 * Run after `tauri build` (or `tauri build --no-bundle`).
 */
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  statSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SAFE_NAME = "SuperManGa";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const tauri = JSON.parse(
  readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"),
);
const version = tauri.version || pkg.version;
const exeName = "supermanga.exe";

const releaseDir = join(root, "src-tauri/target/release");
const exePath = join(releaseDir, exeName);
if (!existsSync(exePath)) {
  console.error(`Missing ${exePath}. Run tauri build first.`);
  process.exit(1);
}

const outDir = join(releaseDir, "bundle/portable");
mkdirSync(outDir, { recursive: true });

const stem = `${SAFE_NAME}-${version}-windows-x64`;
const zipPath = join(outDir, `${stem}.zip`);
const innerExe = `${SAFE_NAME}.exe`;

const stagingRoot = mkdtempSync(join(tmpdir(), "jmr-portable-"));
const packageDir = join(stagingRoot, stem);
mkdirSync(packageDir, { recursive: true });
copyFileSync(exePath, join(packageDir, innerExe));

// Bundle platform pdfium next to the portable exe (resources/ may be absent with --no-bundle).
const pdfiumNames = ["pdfium.dll", "libpdfium.dylib", "libpdfium.so"];
const pdfiumCandidates = [
  join(root, "src-tauri/resources/pdfium"),
  join(releaseDir, "resources/pdfium"),
  join(releaseDir, "pdfium"),
  releaseDir,
];
let pdfiumCopied = false;
for (const dir of pdfiumCandidates) {
  for (const name of pdfiumNames) {
    const src = join(dir, name);
    if (!existsSync(src) || !statSync(src).isFile()) continue;
    if (statSync(src).size < 1_000_000) continue;
    copyFileSync(src, join(packageDir, name));
    console.log(`Bundled pdfium: ${name}`);
    pdfiumCopied = true;
    break;
  }
  if (pdfiumCopied) break;
}
if (!pdfiumCopied) {
  console.warn(
    "Warning: pdfium library not found; encrypted / vector PDFs may fail. Run: npm run fetch:pdfium",
  );
}

const psPath = (p) => p.replace(/'/g, "''");
const zipTmp = `${zipPath}.tmp.zip`;
if (existsSync(zipTmp)) {
  try {
    rmSync(zipTmp);
  } catch {
    // ignore
  }
}

if (process.platform === "win32") {
  // Compress the versioned folder so the zip root is SuperManGa-{ver}-windows-x64/.
  execFileSync(
    "powershell.exe",
    [
      "-NoProfile",
      "-Command",
      `Compress-Archive -LiteralPath '${psPath(packageDir)}' -DestinationPath '${psPath(zipTmp)}' -Force`,
    ],
    { stdio: "inherit" },
  );
} else {
  execFileSync("zip", ["-r", zipTmp, stem], {
    cwd: stagingRoot,
    stdio: "inherit",
  });
}

let finalZip = zipPath;
try {
  if (existsSync(zipPath)) rmSync(zipPath);
  renameSync(zipTmp, zipPath);
} catch {
  finalZip = zipTmp;
  console.warn(`Could not replace ${zipPath}; left at ${zipTmp}`);
}

rmSync(stagingRoot, { recursive: true, force: true });

for (const name of readdirSync(outDir)) {
  const full = join(outDir, name);
  if (!statSync(full).isFile()) continue;
  if (full.toLowerCase() === finalZip.toLowerCase()) continue;
  try {
    rmSync(full);
  } catch {
    // ignore locked leftovers
  }
}

console.log(`Portable zip: ${finalZip}`);
