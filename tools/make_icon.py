"""Cuts the exe and window icon from the X of the emblem.

    py -3 -I tools/make_icon.py

Reads docs/assets/texalith-emblem.png (hand-made, 1762x504) and writes assets/texalith-256.png and assets/texalith.ico.
"""
from pathlib import Path

from PIL import Image

X_BOX = (492, 0, 738, 333)  # the X's columns (E ends at 490, A starts at 739) and the rows above "TEXTURE TOOLS"

root = Path(__file__).resolve().parent.parent
word = Image.open(root / 'docs/assets/texalith-emblem.png').convert('RGBA').crop(X_BOX)
letter = word.crop(word.getchannel('A').point(lambda v: 255 if v > 24 else 0).getbbox())
side = max(letter.size) + 8
icon = Image.new('RGBA', (side, side))
icon.alpha_composite(letter, ((side - letter.width) // 2, (side - letter.height) // 2))
icon = icon.resize((256, 256), Image.LANCZOS)
icon.save(root / 'assets/texalith-256.png', optimize=True)
icon.save(root / 'assets/texalith.ico', sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
print(f'X {letter.size} -> assets/texalith-256.png, assets/texalith.ico')
