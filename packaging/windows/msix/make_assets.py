"""Write the MSIX logo images in Assets/ from ../../icons/dfstudio.png.
Run: python3 make_assets.py (needs Pillow)."""
from pathlib import Path
from PIL import Image

here = Path(__file__).resolve().parent
src = Image.open(here / "../../icons/dfstudio.png").convert("RGBA")
out = here / "Assets"
out.mkdir(exist_ok=True)


def square(name, size):
    src.resize((size, size), Image.LANCZOS).save(out / name)


def wide(name, w, h):
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    icon = src.resize((h, h), Image.LANCZOS)
    img.paste(icon, ((w - h) // 2, 0), icon)
    img.save(out / name)


# Base (scale-100) and scale-200 images; Windows picks the one for the display.
for scale in (100, 200):
    k = scale // 100
    square(f"StoreLogo.scale-{scale}.png", 50 * k)
    square(f"Square44x44Logo.scale-{scale}.png", 44 * k)
    square(f"Square150x150Logo.scale-{scale}.png", 150 * k)
    wide(f"Wide310x150Logo.scale-{scale}.png", 310 * k, 150 * k)
# Taskbar, Start list and file icons at exact pixel sizes, without the tile plate.
for size in (16, 24, 32, 48, 256):
    square(f"Square44x44Logo.targetsize-{size}.png", size)
    square(f"Square44x44Logo.targetsize-{size}_altform-unplated.png", size)
