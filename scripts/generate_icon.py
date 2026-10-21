"""Convert the original PNG to Windows icon sizes. Requires Pillow."""
from pathlib import Path
from PIL import Image

root = Path(__file__).resolve().parents[1]
with Image.open(root / "valtheum.png") as source:
    source.convert("RGBA").save(
        root / "static" / "valtheum.ico",
        format="ICO",
        sizes=[(size, size) for size in (16, 20, 24, 32, 40, 48, 64, 128, 256)],
    )
