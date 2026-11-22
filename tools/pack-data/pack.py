#!/usr/bin/env python3
"""Download third-party ORT / detector / OCR / neural assets and pack zips for
GitHub Release tag `data`.

Output: tools/pack-data/data/{stem}-v{ver}.zip + manifest.json
(schema 2 entries: {version, bytes, sha256})

Examples:
  python pack.py
  python pack.py --only runtime
  python pack.py --only runtime-macos-arm64-cpu,detector-yolo
  python pack.py --version 1.0.0 --ort-version 1.30.0
  HF_MIRROR=https://hf-mirror.com python pack.py --only ocr
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import urllib.request
import zipfile
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent
OUT = ROOT / "data"
CACHE = ROOT / ".cache"
EXPORT_OUT = ROOT.parent / "export-detectors" / "out"

ORT_VERSION_DEFAULT = "1.30.0"
# Official ORT dropped osx-x64 after 1.23.0 (NuGet RID + GitHub tarball); pin Intel Mac separately.
ORT_VERSION_MACOS_X64 = "1.23.0"
ASSET_VERSION_DEFAULT = "1.0.0"

HF = os.environ.get("HF_MIRROR", "https://huggingface.co").rstrip("/")
GH_MIT = "https://github.com/zyddnys/manga-image-translator/releases/download/beta-0.3"

# Runtime packs are per OS/arch (native ORT libs). Key → zip stem `runtime-{key}`.
# Example: windows-x64-cpu → runtime-windows-x64-cpu-v1.30.0.zip
# (os, arch, flavor, nuget_rid) — rid None = fetch from GitHub ORT release tarball.
RUNTIME_TARGETS: list[tuple[str, str, str, str | None]] = [
    ("windows", "x64", "cpu", "win-x64"),
    ("macos", "arm64", "cpu", "osx-arm64"),
    # Last official Intel Mac prebuild (NuGet osx-x64); version via ort_version_for_target().
    ("macos", "x64", "cpu", "osx-x64"),
    ("linux", "x64", "cpu", "linux-x64"),
    ("linux", "arm64", "cpu", "linux-arm64"),
    ("windows", "x64", "cuda12", "win-x64"),
    ("windows", "x64", "cuda13", "win-x64"),
    ("linux", "x64", "cuda12", "linux-x64"),
    ("linux", "x64", "cuda13", "linux-x64"),
]


def ort_version_for_target(os_name: str, arch: str, default: str) -> str:
    if os_name == "macos" and arch == "x64":
        return ORT_VERSION_MACOS_X64
    return default

# Non-runtime section -> key -> list of (dest_name, source)
# source: ("url", url) | ("local", path) | ("nuget", flavor)
PACKS: dict[str, dict[str, list[tuple[str, tuple[str, str]]]]] = {
    "detector": {
        "dbnet": [
            (
                "detect-20241225.onnx",
                ("local", str(EXPORT_OUT / "detect-20241225.onnx")),
            )
        ],
        "ctd": [
            (
                "comictextdetector.pt.onnx",
                ("url", f"{GH_MIT}/comictextdetector.pt.onnx"),
            )
        ],
        "yolo": [("ysgyolo.onnx", ("local", str(EXPORT_OUT / "ysgyolo.onnx")))],
        "saber-yolo": [
            ("saber_yolo.onnx", ("local", str(EXPORT_OUT / "saber_yolo.onnx")))
        ],
    },
    "ocr": {
        "manga-ocr": [
            (
                "encoder_model.onnx",
                (
                    "url",
                    f"{HF}/onnx-community/manga-ocr-base-ONNX/resolve/main/onnx/encoder_model.onnx",
                ),
            ),
            (
                "decoder_model.onnx",
                (
                    "url",
                    f"{HF}/onnx-community/manga-ocr-base-ONNX/resolve/main/onnx/decoder_model.onnx",
                ),
            ),
            (
                "vocab.txt",
                ("url", f"{HF}/kha-white/manga-ocr-base/resolve/main/vocab.txt"),
            ),
            (
                "preprocessor_config.json",
                (
                    "url",
                    f"{HF}/onnx-community/manga-ocr-base-ONNX/resolve/main/preprocessor_config.json",
                ),
            ),
        ],
        "paddle": [
            (
                "det.onnx",
                (
                    "url",
                    f"{HF}/haotruong-hitlab/pp-ocrv6-medium-onnx/resolve/main/PP-OCRv6_det_medium.onnx",
                ),
            ),
            (
                "rec.onnx",
                (
                    "url",
                    f"{HF}/haotruong-hitlab/pp-ocrv6-medium-onnx/resolve/main/PP-OCRv6_rec_medium.onnx",
                ),
            ),
            (
                "keys.txt",
                (
                    "url",
                    f"{HF}/arankays/pp-ocrv6-small-onnx/resolve/main/ppocrv6_dict.txt",
                ),
            ),
        ],
        "mit48px": [
            (
                "ocr-48px-ctc.onnx",
                (
                    "url",
                    f"{HF}/Skepsun/manga-translator-ui-onnx/resolve/main/ocr-48px-ctc.onnx",
                ),
            ),
            (
                "alphabet-all-v5.txt",
                (
                    "url",
                    f"{HF}/Skepsun/manga-translator-ui-onnx/resolve/main/alphabet-all-v5.txt",
                ),
            ),
        ],
        "easyocr": [
            (
                "craft.onnx",
                (
                    "url",
                    f"{HF}/itextresearch/itext-EasyOCR-craft_mlt_25k/resolve/main/itext-EasyOCR-craft_mlt_25k.onnx",
                ),
            ),
            (
                "rec_zh_sim.onnx",
                (
                    "url",
                    f"{HF}/itextresearch/itext-EasyOCR-zh_sim_g2/resolve/main/itext-EasyOCR-zh_sim_g2.onnx",
                ),
            ),
            (
                "rec_japanese.onnx",
                (
                    "url",
                    f"{HF}/itextresearch/itext-EasyOCR-japanese_g2/resolve/main/itext-EasyOCR-japanese_g2.onnx",
                ),
            ),
            (
                "rec_korean.onnx",
                (
                    "url",
                    f"{HF}/itextresearch/itext-EasyOCR-korean_g2/resolve/main/itext-EasyOCR-korean_g2.onnx",
                ),
            ),
            (
                "rec_latin.onnx",
                (
                    "url",
                    f"{HF}/itextresearch/itext-EasyOCR-latin_g2/resolve/main/itext-EasyOCR-latin_g2.onnx",
                ),
            ),
        ],
    },
    "neural": {
        "manga-janai-2x": [
            (
                "__zip__",
                (
                    "url",
                    "https://github.com/the-database/MangaJaNai/releases/download/3.0.0/IllustrationJaNai_V3denoise_onnx.zip",
                ),
            )
        ],
        "real-esrgan-anime-2x": [
            (
                "RealESRGAN_x4plus_anime_6B.onnx",
                (
                    "url",
                    f"{HF}/deepghs/imgutils-models/resolve/main/real_esrgan/RealESRGAN_x4plus_anime_6B.onnx",
                ),
            )
        ],
    },
}


def log(msg: str) -> None:
    print(msg, flush=True)


def download(url: str, dest: Path, force: bool = False) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.is_file() and dest.stat().st_size > 0 and not force:
        log(f"  cache hit {dest.name}")
        return
    log(f"  downloading {url}")
    partial = dest.with_suffix(dest.suffix + ".partial")
    req = urllib.request.Request(url, headers={"User-Agent": "SuperManGa-pack-data/1.0"})
    with urllib.request.urlopen(req, timeout=600) as resp, open(partial, "wb") as out:
        shutil.copyfileobj(resp, out)
    partial.replace(dest)


def runtime_key(os_name: str, arch: str, flavor: str) -> str:
    return f"{os_name}-{arch}-{flavor}"


def nuget_url(flavor: str, version: str, rid: str) -> str:
    ver = version.lower()
    if flavor == "cpu":
        pkg = "microsoft.ml.onnxruntime"
        return f"https://globalcdn.nuget.org/packages/{pkg}.{ver}.nupkg"
    if rid.startswith("win"):
        pkg = "microsoft.ml.onnxruntime.gpu.windows"
    elif rid.startswith("linux"):
        pkg = "microsoft.ml.onnxruntime.gpu.linux"
    else:
        raise ValueError(f"no CUDA NuGet package for rid={rid}")
    if flavor == "cuda12":
        return (
            "https://aiinfra.pkgs.visualstudio.com/PublicPackages/_packaging/"
            f"onnxruntime-cuda-12/nuget/v3/flat2/{pkg}/{ver}/{pkg}.{ver}.nupkg"
        )
    return f"https://globalcdn.nuget.org/packages/{pkg}.{ver}.nupkg"


def github_ort_tarball_url(os_name: str, arch: str, version: str) -> str:
    """Fallback when a RID is missing from the NuGet package (e.g. osx-x64 in 1.30)."""
    ver = version.lstrip("v")
    if os_name == "macos" and arch == "x64":
        name = f"onnxruntime-osx-x86_64-{ver}.tgz"
    elif os_name == "macos" and arch == "arm64":
        name = f"onnxruntime-osx-arm64-{ver}.tgz"
    elif os_name == "linux" and arch == "x64":
        name = f"onnxruntime-linux-x64-{ver}.tgz"
    elif os_name == "linux" and arch == "arm64":
        name = f"onnxruntime-linux-aarch64-{ver}.tgz"
    elif os_name == "windows" and arch == "x64":
        name = f"onnxruntime-win-x64-{ver}.zip"
    else:
        raise ValueError(f"no GitHub ORT tarball for {os_name}-{arch}")
    return f"https://github.com/microsoft/onnxruntime/releases/download/v{ver}/{name}"


def extract_runtime_from_nupkg(nupkg: Path, staging: Path, rid: str) -> None:
    staging.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(nupkg, "r") as zf:
        prefix = f"runtimes/{rid}/native/"
        found = False
        for info in zf.infolist():
            name = info.filename.replace("\\", "/")
            if not name.startswith(prefix) or name.endswith("/"):
                continue
            base = Path(name).name
            if not base:
                continue
            # Skip .lib import libraries
            if base.lower().endswith(".lib"):
                continue
            target = staging / base
            with zf.open(info) as src, open(target, "wb") as dst:
                shutil.copyfileobj(src, dst)
            found = True
        if not found:
            raise RuntimeError(f"no native libs for {rid} in {nupkg.name}")


def _is_ort_native(name: str) -> bool:
    base = Path(name).name
    if not base or base.startswith("."):
        return False
    lower = base.lower()
    if lower.endswith(".lib"):
        return False
    return (
        lower.endswith(".dll")
        or lower.endswith(".dylib")
        or ".so" in lower
    )


def extract_runtime_from_github_archive(archive: Path, staging: Path) -> None:
    """Flatten lib/ natives from official ORT GitHub zip/tgz into staging."""
    import tarfile

    staging.mkdir(parents=True, exist_ok=True)
    written: list[str] = []

    def consider(path_in_archive: str) -> bool:
        parts = path_in_archive.replace("\\", "/").split("/")
        return "lib" in parts and _is_ort_native(path_in_archive)

    is_zip = archive.name.lower().endswith(".zip")
    if is_zip:
        with zipfile.ZipFile(archive, "r") as zf:
            for info in zf.infolist():
                name = info.filename.replace("\\", "/")
                if info.is_dir() or not consider(name):
                    continue
                base = Path(name).name
                with zf.open(info) as src, open(staging / base, "wb") as dst:
                    shutil.copyfileobj(src, dst)
                written.append(base)
    else:
        with tarfile.open(archive, "r:*") as tf:
            for member in tf.getmembers():
                if not member.isfile() or not consider(member.name):
                    continue
                base = Path(member.name).name
                src = tf.extractfile(member)
                if src is None:
                    continue
                with src, open(staging / base, "wb") as dst:
                    shutil.copyfileobj(src, dst)
                written.append(base)
    if not written:
        raise RuntimeError(f"no native ORT libs in {archive.name}")


def zip_dir(src_dir: Path, zip_path: Path) -> None:
    zip_path.parent.mkdir(parents=True, exist_ok=True)
    if zip_path.exists():
        zip_path.unlink()
    with zipfile.ZipFile(zip_path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for path in sorted(src_dir.rglob("*")):
            if path.is_file():
                zf.write(path, path.relative_to(src_dir).as_posix())


def file_sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            chunk = f.read(1024 * 1024)
            if not chunk:
                break
            h.update(chunk)
    return h.hexdigest()


def pack_runtime_one(
    os_name: str,
    arch: str,
    flavor: str,
    rid: str | None,
    version: str,
    *,
    skip_download: bool,
    force: bool,
) -> dict | None:
    key = runtime_key(os_name, arch, flavor)
    stem = f"runtime-{key}"
    folder_name = f"{stem}-v{version}"
    zip_path = OUT / f"{folder_name}.zip"
    staging = CACHE / "staging" / folder_name
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True, exist_ok=True)

    try:
        try:
            if rid is not None:
                url = nuget_url(flavor, version, rid)
                # CPU nupkg is cross-RID; CUDA nupkgs are OS-specific.
                cache_tag = rid.split("-", 1)[0] if flavor != "cpu" else "any"
                cached = CACHE / f"ort-{flavor}-{cache_tag}-{version}.nupkg"
                if not skip_download:
                    download(url, cached, force=force)
                elif not cached.is_file():
                    log(f"  SKIP {stem}: no nupkg cache")
                    return None
                extract_runtime_from_nupkg(cached, staging, rid)
            else:
                url = github_ort_tarball_url(os_name, arch, version)
                cached = CACHE / Path(url).name
                if not skip_download:
                    download(url, cached, force=force)
                elif not cached.is_file():
                    log(f"  SKIP {stem}: no GitHub archive cache")
                    return None
                extract_runtime_from_github_archive(cached, staging)
        except Exception as err:
            log(f"  SKIP {stem}: {err}")
            return None
        zip_dir(staging, zip_path)
        digest = file_sha256(zip_path)
        size = zip_path.stat().st_size
        log(f"  wrote {zip_path.relative_to(ROOT)} ({size} bytes, sha256={digest[:12]}…)")
        return {"version": version, "bytes": size, "sha256": digest}
    finally:
        if staging.exists():
            shutil.rmtree(staging, ignore_errors=True)


def pack_one(
    section: str,
    key: str,
    version: str,
    *,
    skip_download: bool,
    force: bool,
) -> dict | None:
    stem = f"{section}-{key}"
    folder_name = f"{stem}-v{version}"
    zip_path = OUT / f"{folder_name}.zip"
    entries = PACKS[section][key]
    staging = CACHE / "staging" / folder_name
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True, exist_ok=True)

    try:
        for dest_name, (kind, src) in entries:
            if kind == "local":
                local = Path(src)
                if not local.is_file():
                    log(f"  SKIP {stem}: missing local {local}")
                    return None
                shutil.copy2(local, staging / dest_name)
            elif kind == "url":
                if dest_name == "__zip__":
                    cached = CACHE / Path(src).name.split("?")[0]
                    if not skip_download:
                        download(src, cached, force=force)
                    elif not cached.is_file():
                        log(f"  SKIP {stem}: no cache for zip")
                        return None
                    with zipfile.ZipFile(cached, "r") as zf:
                        zf.extractall(staging)
                else:
                    cached = CACHE / f"{stem}-{dest_name}"
                    if not skip_download:
                        download(src, cached, force=force)
                    elif not cached.is_file():
                        log(f"  SKIP {stem}: no cache for {dest_name}")
                        return None
                    shutil.copy2(cached, staging / dest_name)
            else:
                raise ValueError(kind)
        zip_dir(staging, zip_path)
        digest = file_sha256(zip_path)
        size = zip_path.stat().st_size
        log(f"  wrote {zip_path.relative_to(ROOT)} ({size} bytes, sha256={digest[:12]}…)")
        return {"version": version, "bytes": size, "sha256": digest}
    finally:
        if staging.exists():
            shutil.rmtree(staging, ignore_errors=True)


def parse_only(raw: str | None) -> set[str] | None:
    if not raw:
        return None
    parts = {p.strip() for p in raw.split(",") if p.strip()}
    return parts or None


def match_only(only: set[str] | None, section: str, key: str) -> bool:
    if only is None:
        return True
    stem = f"{section}-{key}"
    return section in only or stem in only or key in only


def match_runtime_only(only: set[str] | None, key: str, flavor: str) -> bool:
    if only is None:
        return True
    stem = f"runtime-{key}"
    return (
        "runtime" in only
        or stem in only
        or key in only
        or flavor in only
        or f"runtime-{flavor}" in only
    )


def normalize_entry(raw) -> dict | None:
    """Require {version, bytes, sha256} objects (no bare version strings)."""
    if not isinstance(raw, dict):
        return None
    version = str(raw.get("version", "")).strip()
    if not version:
        return None
    bytes_val = raw.get("bytes", 0)
    if not isinstance(bytes_val, int) or bytes_val < 0:
        bytes_val = 0
    sha = str(raw.get("sha256") or "").strip().lower()
    return {"version": version, "bytes": bytes_val, "sha256": sha}


def write_manifest(entries: dict[str, dict[str, dict]]) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    manifest = {
        "schema": 2,
        "updatedAt": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "runtime": entries.get("runtime", {}),
        "detector": entries.get("detector", {}),
        "ocr": entries.get("ocr", {}),
        "neural": entries.get("neural", {}),
    }
    path = OUT / "manifest.json"
    path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    log(f"wrote {path.relative_to(ROOT)}")


def meta_from_existing_zip(zip_path: Path, version: str) -> dict:
    digest = file_sha256(zip_path)
    size = zip_path.stat().st_size
    log(f"  {zip_path.name} ({size} bytes, sha256={digest[:12]}…)")
    return {"version": version, "bytes": size, "sha256": digest}


def resolve_existing_zip(stem: str, preferred_version: str) -> tuple[str, Path] | None:
    """Prefer `{stem}-v{preferred}.zip`; otherwise the sole `{stem}-v*.zip`."""
    preferred = OUT / f"{stem}-v{preferred_version}.zip"
    if preferred.is_file():
        return preferred_version, preferred
    matches = sorted(OUT.glob(f"{stem}-v*.zip"))
    if not matches:
        return None
    if len(matches) > 1:
        names = ", ".join(p.name for p in matches)
        log(f"  SKIP {stem}: multiple zips, pass --version/--ort-version ({names})")
        return None
    name = matches[0].stem  # e.g. runtime-cpu-v1.30.0
    marker = f"{stem}-v"
    if not name.startswith(marker):
        return None
    return name[len(marker) :], matches[0]


def manifest_from_existing_zips(
    only: set[str] | None,
    *,
    asset_version: str,
    ort_version: str,
) -> dict[str, dict[str, dict]]:
    entries: dict[str, dict[str, dict]] = {
        "runtime": {},
        "detector": {},
        "ocr": {},
        "neural": {},
    }
    for os_name, arch, flavor, _rid in RUNTIME_TARGETS:
        key = runtime_key(os_name, arch, flavor)
        if not match_runtime_only(only, key, flavor):
            continue
        stem = f"runtime-{key}"
        prefer = ort_version_for_target(os_name, arch, ort_version)
        log(f"\n[runtime/{key}]")
        resolved = resolve_existing_zip(stem, prefer)
        if not resolved:
            log(f"  SKIP {stem}: no zip in data/")
            continue
        version, zip_path = resolved
        entries["runtime"][key] = meta_from_existing_zip(zip_path, version)
    for section, keys in PACKS.items():
        for key in keys:
            if not match_only(only, section, key):
                continue
            stem = f"{section}-{key}"
            log(f"\n[{section}/{key}]")
            resolved = resolve_existing_zip(stem, asset_version)
            if not resolved:
                log(f"  SKIP {stem}: no zip in data/")
                continue
            version, zip_path = resolved
            entries[section][key] = meta_from_existing_zip(zip_path, version)
    return entries


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--only",
        help="Comma list: runtime,cpu,detector,ocr,neural or stems like runtime-macos-arm64-cpu",
    )
    parser.add_argument("--version", default=ASSET_VERSION_DEFAULT, help="Non-runtime asset version")
    parser.add_argument("--ort-version", default=ORT_VERSION_DEFAULT, help="Runtime ORT version")
    parser.add_argument("--skip-download", action="store_true")
    parser.add_argument("--force", action="store_true", help="Re-download even if cached")
    parser.add_argument(
        "--manifest-only",
        action="store_true",
        help="Hash existing data/*.zip into manifest.json (no rebuild)",
    )
    args = parser.parse_args()
    only = parse_only(args.only)
    CACHE.mkdir(parents=True, exist_ok=True)
    OUT.mkdir(parents=True, exist_ok=True)

    if args.manifest_only:
        entries = manifest_from_existing_zips(
            only,
            asset_version=args.version,
            ort_version=args.ort_version,
        )
        write_manifest(entries)
        log("\nDone. Re-upload data/manifest.json to GitHub Release tag `data`.")
        log("Also sync src-tauri/resources/data-manifest.json if you ship a new seed.")
        return 0

    entries: dict[str, dict[str, dict]] = {
        "runtime": {},
        "detector": {},
        "ocr": {},
        "neural": {},
    }

    for os_name, arch, flavor, rid in RUNTIME_TARGETS:
        key = runtime_key(os_name, arch, flavor)
        if not match_runtime_only(only, key, flavor):
            continue
        ver = ort_version_for_target(os_name, arch, args.ort_version)
        log(f"\n[runtime/{key}] v{ver}")
        meta = pack_runtime_one(
            os_name,
            arch,
            flavor,
            rid,
            ver,
            skip_download=args.skip_download,
            force=args.force,
        )
        if meta:
            entries["runtime"][key] = meta

    for section, keys in PACKS.items():
        for key in keys:
            if not match_only(only, section, key):
                continue
            ver = args.version
            log(f"\n[{section}/{key}] v{ver}")
            meta = pack_one(
                section,
                key,
                ver,
                skip_download=args.skip_download,
                force=args.force,
            )
            if meta:
                entries[section][key] = meta

    # Merge with existing manifest so partial --only runs keep other keys.
    # Drop legacy runtime keys (cpu / cuda12 without OS-arch prefix).
    existing_path = OUT / "manifest.json"
    if existing_path.is_file():
        try:
            existing = json.loads(existing_path.read_text(encoding="utf-8"))
            for section in entries:
                prev = existing.get(section) or {}
                if isinstance(prev, dict):
                    merged: dict[str, dict] = {}
                    for k, v in prev.items():
                        if section == "runtime" and k in ("cpu", "cuda12", "cuda13"):
                            continue
                        norm = normalize_entry(v)
                        if norm:
                            merged[k] = norm
                    merged.update(entries[section])
                    entries[section] = merged
        except json.JSONDecodeError:
            pass

    write_manifest(entries)
    log("\nDone. Upload tools/pack-data/data/* to GitHub Release tag `data`.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
