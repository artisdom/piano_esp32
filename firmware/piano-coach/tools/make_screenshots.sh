#!/usr/bin/env bash
# README screenshots (doc/images/): the Tab5 UI from the real song library
# (tests/ui_preview, bin `shots`) and the small boards' minimal UI
# (tests/mini_preview), rendered on the PC with the device's Slint UI,
# fonts, engraver and globe. Needs Python 3 with Pillow for the collage.
#
#   tools/make_screenshots.sh            # library: $PIANO_MIDI_DIR or ~/w/piano/midi-piano/assets/midi
set -euo pipefail
app_dir="$(cd "$(dirname "$0")/.." && pwd)"
out="$app_dir/../../doc/images"
mkdir -p "$out"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

(cd "$app_dir/tests/ui_preview" &&
    cargo +stable run -q --release --target x86_64-unknown-linux-gnu --bin shots -- "$out")
(cd "$app_dir/tests/mini_preview" &&
    UI_PREVIEW_DIR="$tmp" CARGO_TARGET_DIR=../ui_preview/target \
    cargo +stable run -q --release --target x86_64-unknown-linux-gnu >/dev/null)

python3 - "$tmp" "$out/small-boards.png" <<'PY'
import sys
from PIL import Image, ImageDraw, ImageFont
src, dst = sys.argv[1], sys.argv[2]
S = 2  # enlarge the tiny panels
def shot(name, round_):
    im = Image.open(f"{src}/mini-{name}.png").convert("RGB")
    im = im.resize((im.width * S, im.height * S), Image.LANCZOS)
    if round_:
        mask = Image.new("L", im.size, 0)
        ImageDraw.Draw(mask).ellipse((0, 0, im.width - 1, im.height - 1), fill=255)
        bg = Image.new("RGB", im.size, (255, 255, 255))
        bg.paste(im, (0, 0), mask)
        im = bg
    return im
bar = [shot(n, False) for n in ("bar-songs", "bar-play", "bar-done")]
rnd = [shot(n, True) for n in ("round-songs", "round-play", "round-done")]
pad = 24
w = max(sum(i.width for i in bar), sum(i.width for i in rnd)) + pad * 4
h = bar[0].height + rnd[0].height + pad * 3 + 60
out = Image.new("RGB", (w, h), (255, 255, 255))
d = ImageDraw.Draw(out)
try:
    f = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 22)
except OSError:
    f = ImageFont.load_default()
d.text((pad, 4), "1.9″ bars (ESP32-1732S019, T-Display-S3; WROVER-KIT 3.2″ at 1.25×)", fill=(40, 40, 40), font=f)
x = pad
for im in bar:
    out.paste(im, (x, 34)); x += im.width + pad
y = 34 + bar[0].height + pad
d.text((pad, y), "1.28″ round touch screen (ESP32-2424S012)", fill=(40, 40, 40), font=f)
x = pad
for im in rnd:
    out.paste(im, (x, y + 30)); x += im.width + pad
out.save(dst, optimize=True)
print(dst)
PY
