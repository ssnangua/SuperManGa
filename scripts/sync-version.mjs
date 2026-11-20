/**
 * Copy package.json version into Cargo.toml, tauri.conf.json, and README download links.
 *
 * Used by `npm version patch|minor|major` (lifecycle: version) so the bump
 * is not limited to package.json. Also: `npm run sync-version`.
 */
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = JSON.parse(readFileSync(join(root, "package.json"), "utf8"))
  .version;
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error(`Invalid package.json version: ${version}`);
  process.exit(1);
}

const changed = [];

function writeIfChanged(rel, next) {
  const path = join(root, rel);
  const prev = readFileSync(path, "utf8");
  if (prev === next) return;
  writeFileSync(path, next);
  changed.push(rel);
}

const tauriPath = "src-tauri/tauri.conf.json";
const tauri = JSON.parse(readFileSync(join(root, tauriPath), "utf8"));
const previous = typeof tauri.version === "string" ? tauri.version : "";
tauri.version = version;
writeIfChanged(tauriPath, `${JSON.stringify(tauri, null, 2)}\n`);

const cargoPath = "src-tauri/Cargo.toml";
const cargo = readFileSync(join(root, cargoPath), "utf8");
const cargoNext = cargo.replace(
  /^(\[package\][\s\S]*?^version = )"[^"]+"/m,
  `$1"${version}"`,
);
if (cargoNext === cargo && !cargo.includes(`version = "${version}"`)) {
  console.error("Could not update [package] version in Cargo.toml");
  process.exit(1);
}
writeIfChanged(cargoPath, cargoNext);

const readmes = ["README.md", "README_en.md", "README_ja.md", "README_ko.md"];
if (previous && previous !== version) {
  const fromTag = `v${previous}`;
  const toTag = `v${version}`;
  const fromFile = `SuperManGa-${previous}-`;
  const toFile = `SuperManGa-${version}-`;
  for (const rel of readmes) {
    const text = readFileSync(join(root, rel), "utf8");
    const next = text.split(fromTag).join(toTag).split(fromFile).join(toFile);
    writeIfChanged(rel, next);
  }
}

if (changed.length === 0) {
  console.log(`Version already ${version}`);
} else {
  console.log(`Synced version ${version}:\n  ${changed.join("\n  ")}`);
  try {
    execFileSync("git", ["add", "--", ...changed], { cwd: root, stdio: "pipe" });
  } catch {
    // Not a git checkout, or git unavailable — files are still written.
  }
}
