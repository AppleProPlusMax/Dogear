import os, io, cairosvg
from PIL import Image, ImageDraw, ImageFont
A = os.path.join(os.path.dirname(__file__), "..", "assets")
INTER = "/usr/share/fonts/truetype/sand-box/google/Inter/Inter-VariableFont_opsz,wght.ttf"
CJK = "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"
def f(p, s, idx=0):
    return ImageFont.truetype(p, s, index=idx) if p.endswith(".ttc") else ImageFont.truetype(p, s)
W, H = 2400, 1500
im = Image.new("RGB", (W, H), "#F5F5F7")
d = ImageDraw.Draw(im)
d.text((120, 90), "App icon candidates", font=f(INTER, 56), fill="#1D1D1F")
d.text((120, 170), "图标候选 · 推荐方案为左侧 Dogear 折角", font=f(CJK, 36), fill="#6E6E73")
items = [("icon-1024.png", "Dogear · 折角", "推荐"), ("icon-alt-sheaf.png", "Sheaf · 叠页", "备选 B"), ("icon-alt-margin.png", "Margin · 页边", "备选 C")]
for i, (fn, name, tag) in enumerate(items):
    ic = Image.open(f"{A}/{fn}").convert("RGBA").resize((600, 600), Image.LANCZOS)
    x = 120 + i * 760
    im.paste(ic, (x, 260), ic)
    d.text((x + 20, 890), name, font=f(CJK, 40), fill="#1D1D1F")
    d.text((x + 20, 950), tag, font=f(CJK, 30), fill="#0A84FF" if i == 0 else "#86868B")
# 小尺寸 + 托盘
d.text((120, 1060), "Small sizes (ico) · 托盘", font=f(INTER, 34), fill="#1D1D1F")
ico = Image.open(f"{A}/icon.ico")
x = 120
for s in (256, 128, 64, 48, 32, 16):
    ico.size = (s, s)
    im2 = ico.convert("RGBA") if ico.size == (s, s) else None
    try:
        ico.size = (s, s); im2 = ico.convert("RGBA")
    except Exception:
        im2 = Image.open(f"{A}/icon-1024.png").convert("RGBA").resize((s, s), Image.LANCZOS)
    im.paste(im2, (x, 1130 + (256 - s)), im2); x += s + 40
# tray 在浅/深色任务栏
for j, (bg, fn) in enumerate([("#F3F3F3", "tray.png"), ("#202020", "tray-white.png")]):
    bx = 1260 + j * 480
    d.rounded_rectangle((bx, 1150, bx + 420, 1330), 28, fill=bg)
    t = Image.open(f"{A}/{fn}").convert("RGBA")
    for k, s in enumerate((16, 24, 32)):
        tt = t.resize((s, s), Image.LANCZOS)
        im.paste(tt, (bx + 50 + k * 100, 1215), tt)
d.text((1260, 1360), "tray.png（浅色任务栏）", font=f(CJK, 26), fill="#6E6E73")
d.text((1740, 1360), "tray-white.png（深色任务栏）", font=f(CJK, 26), fill="#6E6E73")
im.save(f"{A}/icon-preview.png")
