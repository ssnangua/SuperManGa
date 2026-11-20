#!/usr/bin/env python3
"""
Export Ballons/Saber Default DBNet ckpt → ONNX.

Preferred: pass --saber-root to a Saber-Translator checkout so this script can
import TextDetection, load the state_dict, and export with Saber semantics
(input roughly [-1, 1]; output = sigmoid(db) probability map).

Without --saber-root, only succeeds if the ckpt already embeds an nn.Module.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


def _torch_load(weights: Path):
    import torch

    try:
        return torch.load(weights, map_location="cpu", weights_only=True)
    except TypeError:
        return torch.load(weights, map_location="cpu")


def _as_state_dict(obj):
    if isinstance(obj, dict) and "state_dict" in obj and isinstance(obj["state_dict"], dict):
        return obj["state_dict"]
    if isinstance(obj, dict) and obj and all(isinstance(k, str) for k in obj):
        sample = next(iter(obj.values()))
        if hasattr(sample, "shape"):
            return obj
    return None


def _build_from_saber(saber_root: Path):
    root = saber_root.resolve()
    marker = root / "src" / "interfaces" / "default" / "DBNet_resnet34.py"
    if not marker.is_file():
        raise SystemExit(
            f"not a Saber-Translator tree (missing TextDetection): {root}\n"
            "Expected …/Saber-Translator with src/interfaces/default/DBNet_resnet34.py"
        )
    sys.path.insert(0, str(root))
    from src.interfaces.default.DBNet_resnet34 import TextDetection  # type: ignore

    return TextDetection()


def _make_export_wrapper(torch, model):
    """Saber forward returns (db, mask); runtime uses first output as prob map."""

    class DbSigmoid(torch.nn.Module):
        def __init__(self, inner: torch.nn.Module):
            super().__init__()
            self.inner = inner

        def forward(self, x):
            db, _mask = self.inner(x)
            return db.sigmoid()

    return DbSigmoid(model)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--weights", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--imgsz", type=int, default=1536)
    parser.add_argument(
        "--saber-root",
        type=Path,
        default=None,
        help="Path to Saber-Translator checkout (uses TextDetection + load_state_dict)",
    )
    args = parser.parse_args()

    if not args.weights.is_file():
        raise SystemExit(f"weights not found: {args.weights}")

    import torch

    export_model = None

    if args.saber_root is not None:
        model = _build_from_saber(args.saber_root)
        state = _as_state_dict(_torch_load(args.weights))
        if state is None:
            raise SystemExit(f"expected state_dict from {args.weights}")
        missing, unexpected = model.load_state_dict(state, strict=False)
        if missing:
            print(f"warning: missing keys ({len(missing)}): {list(missing)[:8]}…")
        if unexpected:
            print(f"warning: unexpected keys ({len(unexpected)}): {list(unexpected)[:8]}…")
        model.eval()
        export_model = _make_export_wrapper(torch, model)
        export_model.eval()
        print(f"loaded TextDetection from {args.saber_root.resolve()}")
    else:
        obj = _torch_load(args.weights)
        model = None
        if isinstance(obj, torch.nn.Module):
            model = obj
        elif isinstance(obj, dict):
            for key in ("model", "net", "detector"):
                if key in obj and isinstance(obj[key], torch.nn.Module):
                    model = obj[key]
                    break
        if model is None:
            raise SystemExit(
                "Could not find an nn.Module in the ckpt.\n"
                "Pass --saber-root PATH/to/Saber-Translator so the script can\n"
                "construct TextDetection and load this state_dict, e.g.:\n"
                "  python export_default_dbnet.py --weights detect-20241225.ckpt \\\n"
                "    --out out/detect-20241225.onnx --saber-root F:/path/Saber-Translator\n"
                f"ckpt type/keys: {list(obj.keys())[:12] if isinstance(obj, dict) else type(obj)}…"
            )
        model.eval()
        export_model = model

    dummy = torch.zeros(1, 3, args.imgsz, args.imgsz)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    # Keep H/W dynamic when possible; runtime still letterboxes to imgsz×imgsz for
    # fixed exports (legacy TorchScript without dynamic axes).
    export_kwargs = dict(
        input_names=["images"],
        output_names=["output"],
        opset_version=17,
        do_constant_folding=True,
        dynamic_axes={
            "images": {0: "batch", 2: "height", 3: "width"},
            "output": {0: "batch", 2: "out_h", 3: "out_w"},
        },
    )
    # Torch 2.9+ defaults to dynamo=True (needs onnxscript); prefer legacy exporter.
    try:
        torch.onnx.export(
            export_model,
            dummy,
            str(args.out),
            dynamo=False,
            **export_kwargs,
        )
    except TypeError:
        torch.onnx.export(export_model, dummy, str(args.out), **export_kwargs)
    print(f"wrote {args.out}")
    print("Runtime expects input RGB mapped to [-1, 1], letterboxed to imgsz×imgsz.")
    print("Update assets.ts sha256 after hosting this file.")


if __name__ == "__main__":
    main()
