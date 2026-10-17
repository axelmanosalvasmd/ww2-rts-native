#!/usr/bin/env python3
"""Round 17 (poses): a side by side PNG of two frames with a caption over each half (Pillow only).

  compose-pair.py <left.png> <right.png> <out.png> --left "caption" --right "caption" [--crop x0,y0,x1,y1 --scale 2]

--crop takes the same box from both frames (pixels of the frames as saved) and --scale enlarges it, for a detail pair.
"""
import argparse
from PIL import Image, ImageDraw, ImageFont


def font(size):
    for name in ("/usr/share/fonts/TTF/DejaVuSans.ttf", "/usr/share/fonts/dejavu/DejaVuSans.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    return ImageFont.load_default()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("left"); ap.add_argument("right"); ap.add_argument("out")
    ap.add_argument("--left-caption", "--left", dest="lc", default="")
    ap.add_argument("--right-caption", "--right", dest="rc", default="")
    ap.add_argument("--crop", default=""); ap.add_argument("--scale", type=float, default=1.0)
    a = ap.parse_args()
    frames = [Image.open(a.left).convert("RGB"), Image.open(a.right).convert("RGB")]
    if a.crop:
        box = tuple(int(v) for v in a.crop.split(","))
        frames = [f.crop(box) for f in frames]
    if a.scale != 1.0:
        frames = [f.resize((round(f.width * a.scale), round(f.height * a.scale)), Image.LANCZOS) for f in frames]
    w, h = max(f.width for f in frames), max(f.height for f in frames)
    bar = 26
    out = Image.new("RGB", (w * 2 + 8, h + bar), (20, 20, 20))
    d = ImageDraw.Draw(out)
    f = font(15)
    for i, (frame, cap) in enumerate(zip(frames, (a.lc, a.rc))):
        x = i * (w + 8)
        out.paste(frame, (x, bar))
        d.text((x + 8, 5), cap, fill=(235, 235, 235), font=f)
    out.save(a.out)


if __name__ == "__main__":
    main()
