"""產生 n2p-image 的 golden fixtures。

用 Python 版（numpy/scipy 實作）算出參考輸出，讓 Rust 移植能逐像素比對。
scipy 在本機裝不起來，因此以 stub 載入 `inpaint_methods.py`，
只取用不需要 scipy 的函式（`griddata` 本來就要被 IDW 取代）。
"""

import importlib.util
import json
import sys
import types
from pathlib import Path

import numpy as np
from PIL import Image

REPO = Path(__file__).resolve().parents[2]
OUT = REPO / "rust-rewrite" / "assets" / "image-fixtures"
OUT.mkdir(parents=True, exist_ok=True)


# ── stub scipy，讓 inpaint_methods.py 能被載入 ────────────────────────────
if "scipy" not in sys.modules:
    scipy = types.ModuleType("scipy")
    interp = types.ModuleType("scipy.interpolate")

    def griddata(points, values, xi, method="linear"):
        raise NotImplementedError("stub：Rust 版以 IDW 取代")

    interp.griddata = griddata
    scipy.interpolate = interp
    sys.modules["scipy"] = scipy
    sys.modules["scipy.interpolate"] = interp


def load(name, relpath):
    spec = importlib.util.spec_from_file_location(name, REPO / relpath)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


edge = load("edge_diversity", "notebooklm2ppt/utils/edge_diversity.py")
inpaint_methods = load("inpaint_methods", "notebooklm2ppt/utils/inpaint_methods.py")


# ── 合成測試影像 ──────────────────────────────────────────────────────────
W, H = 160, 100
BOX = (60, 40, 100, 55)  # left, top, right, bottom


def make_image():
    """垂直漸層背景 + 一塊會被遮罩的色塊 + 一點雜訊。"""
    rng = np.random.default_rng(20261020)
    img = np.zeros((H, W, 3), dtype=np.uint8)
    for y in range(H):
        t = y / (H - 1)
        img[y, :, :] = [
            int(200 + 40 * t),
            int(210 + 35 * t),
            int(230 + 20 * t),
        ]
    # 左側一個實心圓，用來確認非遮罩區域不被破壞
    yy, xx = np.mgrid[0:H, 0:W]
    circle = (xx - 30) ** 2 + (yy - 30) ** 2 <= 18**2
    img[circle] = [80, 120, 200]
    # 遮罩區放「水印」：半透明白字效果
    l, t, r, b = BOX
    img[t:b, l:r] = (img[t:b, l:r].astype(np.int16) * 0.4 + 255 * 0.6).astype(np.uint8)
    # 細微雜訊（讓 edge diversity 不是完美的 0）
    noise = rng.integers(-3, 4, size=(H, W, 3), dtype=np.int16)
    return np.clip(img.astype(np.int16) + noise, 0, 255).astype(np.uint8)


def make_mask():
    mask = np.zeros((H, W), dtype=np.uint8)
    l, t, r, b = BOX
    mask[t:b, l:r] = 255
    return mask


def run_method(img, mask, method):
    """複現 image_inpainter.inpaint_image 的分派邏輯，座標改由參數給。"""
    l, t, r, b = BOX
    diversity, fill_color = edge.compute_edge_diversity_numpy(img, l, t, r, b)

    if diversity < 0.1 or method == "background":
        out = img.copy()
        out[t:b, l:r] = fill_color
        return out
    if method == "onion":
        return inpaint_methods.inpaint_numpy_onion(img, mask)
    if method == "background_smooth":
        return inpaint_methods.inpaint_manual(img, mask, fill_color, max_iter=100)
    if method == "edge_mean_smooth":
        avg = edge.compute_edge_average_color(img, l, t, r, b)
        return inpaint_methods.inpaint_manual(img, mask, avg, max_iter=100)
    raise ValueError(method)


def main():
    img = make_image()
    mask = make_mask()
    Image.fromarray(img).save(OUT / "input.png")

    l, t, r, b = BOX
    diversity, fill_color = edge.compute_edge_diversity_numpy(img, l, t, r, b)
    average_color = edge.compute_edge_average_color(img, l, t, r, b)
    diversity20, fill20 = edge.compute_edge_diversity_numpy(img, l, t, r, b, tolerance=20)

    metrics = {
        "box": {"left": l, "top": t, "right": r, "bottom": b},
        "size": {"width": W, "height": H},
        "edge_diversity_default": float(diversity),
        "edge_diversity_fill_color": [int(v) for v in fill_color],
        "edge_diversity_tolerance20": float(diversity20),
        "edge_diversity_tolerance20_fill_color": [int(v) for v in fill20],
        "edge_average_color": [int(v) for v in average_color],
    }
    (OUT / "expected_metrics.json").write_text(json.dumps(metrics, indent=2, ensure_ascii=False))

    for method in ("background", "background_smooth", "edge_mean_smooth", "onion"):
        out = run_method(img, mask, method)
        Image.fromarray(out).save(OUT / f"expected_{method}.png")
        print(f"{method}: 已產生")

    print(json.dumps(metrics, indent=2, ensure_ascii=False))
    print(f"\n輸出目錄：{OUT}")


if __name__ == "__main__":
    main()
