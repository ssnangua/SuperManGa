#!/usr/bin/env python3
"""Download third-party ORT / detector / OCR / neural assets and pack zips for
GitHub Release tag `data`.

Output: tools/pack-data/data/{stem}-v{ver}.zip + manifest.json
(schema 2 entries: {version, bytes, sha256})

Examples:
  python pack.py
  python pack.py --only runtime,detector-yolo
  python pack.py --version 1.0.0 --ort-version 1.30.0
  HF_MIRROR=https://hf-mirror.com python pack.py --only ocr
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import sys
import urllib.request
import zipfile
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent
OUT = ROOT / "data"
CACHE = ROOT / ".cache"
EXPORT_OUT = ROOT.parent / "export-detectors" / "out"

ORT_VERSION_DEFAULT = "1.30.0"
ASSET_VERSION_DEFAULT = "1.0.0"

HF = os.environ.get("HF_MIRROR", "https://huggingface.co").rstrip("/")
GH_MIT = "https://github.com/zyddnys/manga-image-translator/releases/download/beta-0.3"

# section -> key -> list of (dest_name, source)
# source: ("url", url) | ("local", path) | ("nuget", flavor)
PACKS: dict[str, dict[str, list[tuple[str, tuple[str, str]]]]] = {
    "runtime": {
        "cpu": [("__runtime__", ("nuget", "cpu"))],
        "cuda12": [("__runtime__", ("nuget", "cuda12"))],
        "cuda13": [("__runtime__", ("nuget", "cuda13"))],
    },
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


def nuget_url(flavor: str, version: str) -> str:
    ver = version.lower()
    if flavor == "cpu":
        pkg = "microsoft.ml.onnxruntime"
        return f"https://globalcdn.nuget.org/packages/{pkg}.{ver}.nupkg"
    pkg = (
        "microsoft.ml.onnxruntime.gpu.windows"
        if sys.platform.startswith("win")
        else "microsoft.ml.onnxruntime.gpu.linux"
    )
    if flavor == "cuda12":
        return (
            "https://aiinfra.pkgs.visualstudio.com/PublicPackages/_packaging/"
            f"onnxruntime-cuda-12/nuget/v3/flat2/{pkg}/{ver}/{pkg}.{ver}.nupkg"
        )
    return f"https://globalcdn.nuget.org/packages/{pkg}.{ver}.nupkg"


def host_rid() -> str:
    if sys.platform.startswith("win"):
        return "win-x64"
    if sys.platform == "darwin":
        return "osx-arm64" if os.uname().machine == "arm64" else "osx-x64"
    return "linux-x64"


def extract_runtime_dlls(nupkg: Path, staging: Path) -> None:
    rid = host_rid()
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
            elif kind == "nuget":
                flavor = src
                url = nuget_url(flavor, version)
                cached = CACHE / f"ort-{flavor}-{version}.nupkg"
                if not skip_download:
                    download(url, cached, force=force)
                elif not cached.is_file():
                    log(f"  SKIP {stem}: no nupkg cache")
                    return None
                extract_runtime_dlls(cached, staging)
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
    for section, keys in PACKS.items():
        for key in keys:
            if not match_only(only, section, key):
                continue
            stem = f"{section}-{key}"
            preferred = ort_version if section == "runtime" else asset_version
            log(f"\n[{section}/{key}]")
            resolved = resolve_existing_zip(stem, preferred)
            if not resolved:
                log(f"  SKIP {stem}: no zip in data/")
                continue
            version, zip_path = resolved
            entries[section][key] = meta_from_existing_zip(zip_path, version)
    return entries


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--only", help="Comma list: runtime,detector,ocr,neural or full stems")
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

    for section, keys in PACKS.items():
        for key in keys:
            if not match_only(only, section, key):
                continue
            ver = args.ort_version if section == "runtime" else args.version
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
    existing_path = OUT / "manifest.json"
    if existing_path.is_file():
        try:
            existing = json.loads(existing_path.read_text(encoding="utf-8"))
            for section in entries:
                prev = existing.get(section) or {}
                if isinstance(prev, dict):
                    merged: dict[str, dict] = {}
                    for k, v in prev.items():
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
