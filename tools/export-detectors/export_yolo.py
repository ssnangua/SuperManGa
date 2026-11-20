#!/usr/bin/env python3
"""Export Ultralytics YOLO detect / OBB weights to ONNX for ORT."""

from __future__ import annotations

import argparse
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--weights", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--imgsz", type=int, default=640)
    parser.add_argument("--obb", action="store_true", help="OBB model (SaberYOLO)")
    args = parser.parse_args()

    if not args.weights.is_file():
        raise SystemExit(f"weights not found: {args.weights}")

    from ultralytics import YOLO

    model = YOLO(str(args.weights))
    args.out.parent.mkdir(parents=True, exist_ok=True)
    # Ultralytics writes beside weights by default; force our path via export then rename.
    exported = model.export(
        format="onnx",
        imgsz=args.imgsz,
        simplify=True,
        opset=17,
        dynamic=False,
    )
    exported_path = Path(str(exported))
    if exported_path.resolve() != args.out.resolve():
        args.out.write_bytes(exported_path.read_bytes())
        if exported_path != args.weights.with_suffix(".onnx"):
            # keep ultralytics artifact if same folder; otherwise leave it
            pass
    print(f"wrote {args.out} (obb={args.obb})")
    print("Update assets.ts sha256 after hosting this file.")


if __name__ == "__main__":
    main()
