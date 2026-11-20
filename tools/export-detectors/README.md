# Export text detectors to ONNX

Runtime uses ORT only. Convert Saber / Ballons PyTorch weights once, then pack them with [`tools/pack-data/`](../pack-data/) into Release tag `data` (e.g. `detector-dbnet-v1.0.0.zip`).

## Requirements

- Python 3.10+
- `pip install -r requirements.txt`
- Source weights (not shipped in this repo):
  - `detect-20241225.ckpt` — Saber/Ballons `models/default/`
  - `ysgyolo_*.pt` — Saber/Ballons `models/yolo/`
  - `saber_yolo.pt` — Saber `models/saber_yolo/`

## Commands

```bash
# YSGYolo / SaberYOLO (ultralytics)
python export_yolo.py --weights path/to/ysgyolo_1.2_OS1.0.pt --out out/ysgyolo.onnx
python export_yolo.py --weights path/to/saber_yolo.pt --out out/saber_yolo.onnx --obb

# Default DBNet — pass a local Saber-Translator checkout for TextDetection
python export_default_dbnet.py --weights path/to/detect-20241225.ckpt --out out/detect-20241225.onnx --saber-root path/to/Saber-Translator
```

`--saber-root` must contain `src/interfaces/default/DBNet_resnet34.py`. Export wraps `sigmoid(db)` so the ONNX output is a probability map; runtime input is RGB in **[-1, 1]** (same as Saber).


Put `out/*.onnx` where `tools/pack-data/pack.py` expects them, then run the packer:

| pack stem | file |
|---|---|
| `detector-dbnet` | `detect-20241225.onnx` |
| `detector-yolo` | `ysgyolo.onnx` |
| `detector-saber-yolo` | `saber_yolo.onnx` |

`detector-ctd` is downloaded by the packer from the MIT release (no local export).
