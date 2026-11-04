"""Draw the DeltaF Studio icon (a T-s saturation dome with a Rankine-like cycle)
and write the PNG, Windows .ico, macOS .icns and the raw RGBA used for the
window icon. Run: python3 make_icon.py (needs Pillow)."""
import math
from PIL import Image, ImageDraw

S = 1024
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)
d.rounded_rectangle([40, 40, S - 40, S - 40], radius=200, fill=(22, 52, 96, 255))
# saturation dome
dome = [(180 + 664 * t, 820 - 560 * math.sin(math.pi * t) ** 0.9) for t in [i / 100 for i in range(101)]]
d.line(dome, fill=(150, 190, 235, 255), width=26, joint="curve")
# Rankine cycle on the T-s plane: pump, heating and boiling at constant
# pressure, superheat, expansion, condensation back to saturated liquid
def dome_x(y, left):
    ts = [i / 2000 for i in range(2001)]
    pts = [(180 + 664 * t, 820 - 560 * math.sin(math.pi * t) ** 0.9) for t in ts]
    side = [p for p in pts if (p[0] < 512) == left]
    return min(side, key=lambda p: abs(p[1] - y))[0]
y_lo, y_hi = 700, 400
l_lo, l_hi, r_hi = dome_x(y_lo, True), dome_x(y_hi, True), dome_x(y_hi, False)
cyc = [(l_lo, y_lo), (l_lo + 6, y_lo - 30), (l_hi, y_hi), (r_hi, y_hi), (r_hi + 70, 190), (r_hi + 110, y_lo), (l_lo, y_lo)]
d.line(cyc, fill=(255, 150, 40, 255), width=34, joint="curve")
for p in [cyc[0], cyc[2], cyc[3], cyc[4], cyc[5]]:
    d.ellipse([p[0] - 30, p[1] - 30, p[0] + 30, p[1] + 30], fill=(255, 255, 255, 255))
img.save("dfstudio.png")
img.resize((256, 256), Image.LANCZOS).save("dfstudio.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
img.save("dfstudio.icns")
small = img.resize((64, 64), Image.LANCZOS)
open("icon_64.rgba", "wb").write(small.tobytes())
