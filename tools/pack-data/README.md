# Pack data Release assets

Download third-party models / ORT natives and build versioned zips for the GitHub Release tag **`data`**.

```
tools/pack-data/
├── data/          # output zips + manifest.json (gitignored)
├── .cache/        # download cache (gitignored)
├── pack.py
└── README.md
```

## Usage

```bash
cd tools/pack-data
python pack.py
python pack.py --only runtime          # all OS/arch CPU + CUDA packs
python pack.py --only cpu              # CPU packs only (all platforms)
python pack.py --only runtime-macos-arm64-cpu
python pack.py --only detector-yolo,detector-dbnet
# Already have zips — only rebuild manifest (bytes + sha256):
python pack.py --manifest-only
# Hugging Face mirror for OCR/neural pulls:
# Windows PowerShell:
$env:HF_MIRROR="https://hf-mirror.com"; python pack.py --only ocr,neural
```

**Runtime** zips are per OS/arch (native libs), e.g. `runtime-windows-x64-cpu-v1.30.0.zip`, `runtime-macos-arm64-cpu-v1.30.0.zip`. Detector / OCR / neural **ONNX** packs stay cross-platform (one zip for all OS).

`runtime-macos-x64-cpu` is pinned to **ORT 1.23.0** (last official NuGet `osx-x64` / GitHub `onnxruntime-osx-x86_64`); other platforms default to `--ort-version` (1.30.0).

Do **not** re-pack from `.cache` just to refresh hashes: zip metadata can change `sha256` even when file contents are the same. Prefer `--manifest-only` against the uploaded zips.

Detectors **dbnet / yolo / saber-yolo** prefer ONNX under [`../export-detectors/out/`](../export-detectors/out/). Export those first if missing.

## Manifest entry

Every pack is an object (schema 2):

```json
"yolo": {
  "version": "1.0.0",
  "bytes": 12345678,
  "sha256": "…"
}
```

`pack.py` fills `bytes` / `sha256` from the zip. The app uses `bytes` for UI size and verifies `sha256` (and size) on download. Seed may use `bytes: 0` / `sha256: ""` until a real pack is uploaded.

## Upload

1. Create or edit Release tag `data` on `ssnangua/SuperManGa`.
2. Upload every `data/*.zip` plus `data/manifest.json` (overwrite manifest; keep old versioned zips).
3. App seed: keep [`src-tauri/resources/data-manifest.json`](../../src-tauri/resources/data-manifest.json) in sync with the released manifest.
