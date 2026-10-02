import os, sys
from playwright.sync_api import sync_playwright
root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
url = "file://" + root + "/prototype/index.html"
with sync_playwright() as p:
    b = p.chromium.launch()
    for theme, name in (("light", "ui-light.png"), ("dark", "ui-dark.png")):
        pg = b.new_page(viewport={"width": 840, "height": 1040}, device_scale_factor=2)
        pg.goto(f"{url}?theme={theme}")
        pg.wait_for_timeout(400)
        pg.screenshot(path=f"{root}/assets/{name}")
    b.close()
