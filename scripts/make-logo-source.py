#!/usr/bin/env python3
"""
Auto-center a logo source image into a 1024x1024 PNG suitable for icns
generation by scripts/build-macos.sh.

Detects the logo's bounding box by thresholding against the corner background
colour, squares it up with padding, and pastes onto a square canvas filled
with the same background. The build script consumes this as the icns source.

Usage:
  scripts/make-logo-source.py SOURCE [--out assets/icon.png] [--padding 0.18]
"""
import argparse
import sys
from pathlib import Path

try:
    from PIL import Image, ImageChops
except ImportError as e:
    sys.exit(f"missing dep: {e} (need Pillow: pip3 install pillow)")


def average_corner_color(img: Image.Image) -> tuple[int, int, int]:
    """Sample 8 patches around the edges and return their pixelwise mean."""
    w, h = img.size
    sw = max(20, min(w, h) // 30)
    patches = [
        img.crop((0, 0, sw, sw)),
        img.crop((w - sw, 0, w, sw)),
        img.crop((0, h - sw, sw, h)),
        img.crop((w - sw, h - sw, w, h)),
        img.crop((0, h // 2 - sw // 2, sw, h // 2 + sw // 2)),
        img.crop((w - sw, h // 2 - sw // 2, w, h // 2 + sw // 2)),
        img.crop((w // 2 - sw // 2, 0, w // 2 + sw // 2, sw)),
        img.crop((w // 2 - sw // 2, h - sw, w // 2 + sw // 2, h)),
    ]
    n = len(patches)
    rs = sum(p.resize((1, 1)).getpixel((0, 0))[0] for p in patches) // n
    gs = sum(p.resize((1, 1)).getpixel((0, 0))[1] for p in patches) // n
    bs = sum(p.resize((1, 1)).getpixel((0, 0))[2] for p in patches) // n
    return rs, gs, bs


def auto_center(src: Path, dst: Path, padding: float, out_size: int) -> None:
    img = Image.open(src).convert("RGB")
    w, h = img.size

    bg_color = average_corner_color(img)
    bg_brightness = max(bg_color)

    # Reduce to per-pixel max channel so logo glow stands out from the dark
    # background regardless of which RGB channel dominates the logo.
    r, g, b = img.split()
    max_chan = ImageChops.lighter(ImageChops.lighter(r, g), b)

    # Threshold to a binary mask, then find its bbox.
    threshold = min(255, bg_brightness + 25)
    mask = max_chan.point(lambda v, t=threshold: 255 if v > t else 0, mode="L")
    bbox = mask.getbbox()
    if not bbox:
        sys.exit("could not detect any non-background content in source")

    x0, y0, x1, y1 = bbox
    cx, cy = (x0 + x1) // 2, (y0 + y1) // 2
    span = max(x1 - x0, y1 - y0)
    side = int(span * (1 + padding * 2))

    print(
        f"  source: {w}x{h}  bg≈rgb{bg_color} thresh={threshold}\n"
        f"  bbox: ({x0},{y0})-({x1},{y1})  center=({cx},{cy})  span={span}  side={side}"
    )

    canvas = Image.new("RGB", (side, side), bg_color)
    src_x0, src_y0 = cx - side // 2, cy - side // 2
    src_x1, src_y1 = src_x0 + side, src_y0 + side
    sx0, sy0 = max(src_x0, 0), max(src_y0, 0)
    sx1, sy1 = min(src_x1, w), min(src_y1, h)
    canvas.paste(img.crop((sx0, sy0, sx1, sy1)), (sx0 - src_x0, sy0 - src_y0))

    canvas = canvas.resize((out_size, out_size), Image.LANCZOS)
    dst.parent.mkdir(parents=True, exist_ok=True)
    canvas.save(dst, format="PNG")
    print(f"  wrote: {dst} ({out_size}x{out_size})")


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("source", type=Path)
    p.add_argument("--out", type=Path, default=Path("assets/icon.png"))
    p.add_argument("--padding", type=float, default=0.18,
                   help="extra space around bbox as fraction of bbox span (default 0.18)")
    p.add_argument("--size", type=int, default=1024)
    args = p.parse_args()
    auto_center(args.source, args.out, args.padding, args.size)


if __name__ == "__main__":
    main()
