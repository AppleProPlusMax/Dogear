"""生成图标：Dogear（推荐）+ 两个备选（Sheaf / Margin）。运行：python make_icons.py"""
import math, os, io
import cairosvg
from PIL import Image, ImageDraw, ImageFont

OUT = os.path.join(os.path.dirname(__file__), "..", "assets")
os.makedirs(OUT, exist_ok=True)

def squircle(cx, cy, half, n=5.0, pts=480):
    d = []
    for i in range(pts):
        t = 2 * math.pi * i / pts
        c, s = math.cos(t), math.sin(t)
        x = cx + half * math.copysign(abs(c) ** (2 / n), c)
        y = cy + half * math.copysign(abs(s) ** (2 / n), s)
        d.append(f"{x:.2f},{y:.2f}")
    return "M" + " L".join(d) + " Z"

SQ = squircle(512, 512, 448)  # 896 x 896，四周各留 64（给阴影）

def wrap(defs, body, shadow=True):
    sh = '<path d="%s" fill="#1B2150" opacity=".28" filter="url(#sh)" transform="translate(0,14)"/>' % SQ if shadow else ""
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
<defs>
<filter id="sh" x="-20%" y="-20%" width="140%" height="150%"><feGaussianBlur stdDeviation="22"/></filter>
<clipPath id="clipSq"><path d="{SQ}"/></clipPath>
{defs}
</defs>
{sh}
<g clip-path="url(#clipSq)">
{body}
</g>
<path d="{SQ}" fill="none" stroke="#fff" stroke-opacity=".22" stroke-width="3"/>
</svg>'''

# ---------- A. Dogear 折角（推荐）----------
def dogear():
    x0, y0, x1, y1, r, f = 268, 200, 756, 836, 54, 176
    sheet = (f"M{x0+r},{y0} H{x1-f} L{x1},{y0+f} V{y1-r} A{r},{r} 0 0 1 {x1-r},{y1} "
             f"H{x0+r} A{r},{r} 0 0 1 {x0},{y1-r} V{y0+r} A{r},{r} 0 0 1 {x0+r},{y0} Z")
    # 折角三角：三个顶点略圆润
    flap = f"M{x1-f},{y0} L{x1},{y0+f} H{x1-f-0} Z"
    defs = '''
<linearGradient id="bg" x1="0" y1="0" x2="0.35" y2="1">
  <stop offset="0" stop-color="#9BB0FF"/><stop offset=".55" stop-color="#6A7BFF"/><stop offset="1" stop-color="#4A52EA"/>
</linearGradient>
<radialGradient id="glow" cx=".3" cy=".08" r=".8">
  <stop offset="0" stop-color="#fff" stop-opacity=".42"/><stop offset=".6" stop-color="#fff" stop-opacity="0"/>
</radialGradient>
<linearGradient id="paper" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="#FFFFFF"/><stop offset="1" stop-color="#EEF1FF"/>
</linearGradient>
<linearGradient id="fold" x1="0" y1="0" x2="1" y2="1">
  <stop offset="0" stop-color="#FFFFFF"/><stop offset="1" stop-color="#B9C5FF"/>
</linearGradient>
<linearGradient id="acc" x1="0" y1="0" x2="1" y2="0">
  <stop offset="0" stop-color="#6A7BFF"/><stop offset="1" stop-color="#8F6BFF"/>
</linearGradient>
<filter id="ps" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="20"/></filter>
<filter id="fs" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="9"/></filter>
'''
    body = f'''
<rect width="1024" height="1024" fill="url(#bg)"/>
<rect width="1024" height="1024" fill="url(#glow)"/>
<!-- 后一张纸：暗示历史 -->
<g transform="rotate(-7 512 540)">
  <rect x="{x0+6}" y="{y0+34}" width="{x1-x0-12}" height="{y1-y0-10}" rx="{r}" fill="#fff" opacity=".38"/>
</g>
<!-- 纸张阴影 -->
<path d="{sheet}" fill="#1B2150" opacity=".30" filter="url(#ps)" transform="translate(0,26)"/>
<path d="{sheet}" fill="url(#paper)"/>
<!-- 文字行 -->
<rect x="{x0+62}" y="{y0+230}" width="250" height="34" rx="17" fill="url(#acc)"/>
<rect x="{x0+62}" y="{y0+306}" width="{x1-x0-124}" height="26" rx="13" fill="#D6DCF6"/>
<rect x="{x0+62}" y="{y0+372}" width="{x1-x0-124-70}" height="26" rx="13" fill="#D6DCF6"/>
<rect x="{x0+62}" y="{y0+438}" width="{x1-x0-124-150}" height="26" rx="13" fill="#D6DCF6"/>
<!-- 折角 -->
<path d="M{x1-f},{y0} L{x1},{y0+f} L{x1-f},{y0+f} Z" fill="#1B2150" opacity=".28" filter="url(#fs)" transform="translate(-6,10)"/>
<path d="M{x1-f},{y0} L{x1},{y0+f} L{x1-f},{y0+f} Z" fill="url(#fold)" stroke-linejoin="round" stroke="url(#fold)" stroke-width="14"/>
'''
    return wrap(defs, body)

# ---------- B. Sheaf 叠页（备选）：三张错落卡片，石墨 → 紫灰 ----------
def sheaf():
    defs = '''
<linearGradient id="bg" x1="0" y1="0" x2="0.3" y2="1"><stop offset="0" stop-color="#3A3F52"/><stop offset="1" stop-color="#14161F"/></linearGradient>
<radialGradient id="glow" cx=".3" cy=".05" r=".9"><stop offset="0" stop-color="#fff" stop-opacity=".22"/><stop offset=".6" stop-color="#fff" stop-opacity="0"/></radialGradient>
<linearGradient id="c1" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#FFB4A0"/><stop offset="1" stop-color="#FF7E9B"/></linearGradient>
<linearGradient id="c2" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#9FC4FF"/><stop offset="1" stop-color="#6D8DFF"/></linearGradient>
<linearGradient id="c3" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#FFFFFF"/><stop offset="1" stop-color="#E8EAF4"/></linearGradient>
<filter id="ps" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="16"/></filter>
'''
    def card(dy, rot, fill, op=1):
        return f'''<g transform="rotate({rot} 512 560) translate(0 {dy})">
<rect x="262" y="272" width="500" height="470" rx="64" fill="#000" opacity=".35" filter="url(#ps)" transform="translate(0,22)"/>
<rect x="262" y="272" width="500" height="470" rx="64" fill="url(#{fill})" opacity="{op}"/></g>'''
    body = f'''<rect width="1024" height="1024" fill="url(#bg)"/><rect width="1024" height="1024" fill="url(#glow)"/>
{card(-70,-12,'c1')}{card(-30,6,'c2')}{card(20,0,'c3')}
<rect x="330" y="560" width="220" height="32" rx="16" fill="#6D8DFF"/>
<rect x="330" y="630" width="364" height="26" rx="13" fill="#CDD2E6"/>'''
    return wrap(defs, body)

# ---------- C. Margin 页边（备选）：纸 + 页边竖线 + 光标 ----------
def margin():
    defs = '''
<linearGradient id="bg" x1="0" y1="0" x2="0.4" y2="1"><stop offset="0" stop-color="#8FE3C8"/><stop offset="1" stop-color="#2FB7A0"/></linearGradient>
<radialGradient id="glow" cx=".3" cy=".05" r=".9"><stop offset="0" stop-color="#fff" stop-opacity=".4"/><stop offset=".6" stop-color="#fff" stop-opacity="0"/></radialGradient>
<linearGradient id="paper" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff"/><stop offset="1" stop-color="#EEF9F6"/></linearGradient>
<filter id="ps" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="18"/></filter>
'''
    body = '''<rect width="1024" height="1024" fill="url(#bg)"/><rect width="1024" height="1024" fill="url(#glow)"/>
<rect x="262" y="210" width="500" height="620" rx="64" fill="#0B4A40" opacity=".3" filter="url(#ps)" transform="translate(0,24)"/>
<rect x="262" y="210" width="500" height="620" rx="64" fill="url(#paper)"/>
<rect x="388" y="210" width="8" height="620" fill="#2FB7A0" opacity=".55"/>
<rect x="446" y="330" width="250" height="30" rx="15" fill="#CFE6E0"/>
<rect x="446" y="400" width="200" height="30" rx="15" fill="#CFE6E0"/>
<rect x="446" y="470" width="270" height="30" rx="15" fill="#CFE6E0"/>
<rect x="446" y="540" width="140" height="30" rx="15" fill="#2FB7A0"/>
<rect x="602" y="520" width="10" height="70" rx="5" fill="#1D8F7C"/>'''
    return wrap(defs, body)

# ---------- 托盘单色图标 ----------
def tray(color="#000000"):
    # 32x32 viewBox，纸张 + 折角，镂空文字行；纯单色，便于 Windows 浅/深任务栏
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
<path fill="{color}" fill-rule="evenodd" d="M9 3 H20 L26 9 V26 A3 3 0 0 1 23 29 H9 A3 3 0 0 1 6 26 V6 A3 3 0 0 1 9 3 Z
M11 15 H21 A1.2 1.2 0 0 1 21 17.4 H11 A1.2 1.2 0 0 1 11 15 Z
M11 20.5 H18 A1.2 1.2 0 0 1 18 22.9 H11 A1.2 1.2 0 0 1 11 20.5 Z"/>
<path fill="{color}" opacity=".45" d="M20 3 L26 9 H22 A2 2 0 0 1 20 7 Z"/>
</svg>'''

def png(svg, path, size):
    cairosvg.svg2png(bytestring=svg.encode(), write_to=path, output_width=size, output_height=size)

if __name__ == "__main__":
    A, B, C = dogear(), sheaf(), margin()
    open(f"{OUT}/icon.svg", "w").write(A)
    open(f"{OUT}/icon-alt-sheaf.svg", "w").write(B)
    open(f"{OUT}/icon-alt-margin.svg", "w").write(C)
    png(A, f"{OUT}/icon-1024.png", 1024)
    png(B, f"{OUT}/icon-alt-sheaf.png", 1024)
    png(C, f"{OUT}/icon-alt-margin.png", 1024)
    open(f"{OUT}/tray.svg", "w").write(tray())
    open(f"{OUT}/tray-white.svg", "w").write(tray("#FFFFFF"))
    png(tray(), f"{OUT}/tray.png", 32)
    png(tray("#FFFFFF"), f"{OUT}/tray-white.png", 32)
    png(tray(), f"{OUT}/tray-128.png", 128)  # 预览用
    # ICO：Windows 图标不留 macOS 式大边距 -> 从 1024 图中裁掉外围留白（squircle 占 896，裁成 ~ 920）
    base = Image.open(f"{OUT}/icon-1024.png").convert("RGBA")
    crop = base.crop((48, 40, 976, 968)).resize((1024, 1024), Image.LANCZOS)
    sizes = [16, 32, 48, 64, 128, 256]
    crop.save(f"{OUT}/icon.ico", sizes=[(s, s) for s in sizes])
    print("done")
