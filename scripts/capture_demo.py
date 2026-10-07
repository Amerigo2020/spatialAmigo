"""Capture a screenshot and an animated GIF of the web demo, headless.

    scripts/build_web.sh
    python3 scripts/capture_demo.py            # needs playwright + pillow, uses the bundled Chromium

Writes docs/media/demo.png and docs/media/demo.gif.
"""

from __future__ import annotations

import functools
import http.server
import io
import sys
import threading
from pathlib import Path

from PIL import Image
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "web" / "dist"
OUT = ROOT / "docs" / "media"

SEQUENCE = [
    ("Living room", "the armchair to the left of the sofa"),
    ("Living room", "the plant to the right of the tv"),
    ("Living room", "the plant to the left of the tv from my perspective"),
    ("Living room", "the mug closest to the bowl"),
    ("Tabletop", "the mug left of the laptop"),
    ("Tabletop", "the mug to the left of the chair from my perspective"),
]


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args) -> None:  # noqa: D102
        pass


def serve() -> http.server.ThreadingHTTPServer:
    handler = functools.partial(QuietHandler, directory=str(DIST))
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv


def main() -> int:
    if not (DIST / "index.html").exists():
        print("web/dist missing, run scripts/build_web.sh first", file=sys.stderr)
        return 1
    OUT.mkdir(parents=True, exist_ok=True)
    srv = serve()
    url = f"http://127.0.0.1:{srv.server_address[1]}/index.html"
    errors: list[str] = []
    with sync_playwright() as p:
        browser = p.chromium.launch(args=["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"])
        page = browser.new_page(viewport={"width": 1400, "height": 820}, device_scale_factor=1, color_scheme="light")
        page.on("console", lambda m: errors.append(m.text) if m.type == "error" else None)
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.goto(url)
        page.wait_for_function("window.__demo !== undefined")
        page.evaluate("window.__demo.ready")
        page.wait_for_timeout(300)

        page.evaluate("window.__demo.run('the plant to the right of the tv')")
        page.wait_for_timeout(200)
        page.screenshot(path=str(OUT / "demo.png"))

        frames: list[Image.Image] = []
        current = None
        for scene, text in SEQUENCE:
            if scene != current:
                page.evaluate(f"window.__demo.loadScene({scene!r})")
                current = scene
            page.evaluate(f"window.__demo.run({text!r})")
            for i in range(5):
                if i:
                    page.evaluate("window.__demo.orbit(2.5)")
                png = page.screenshot()
                img = Image.open(io.BytesIO(png)).convert("RGB")
                img = img.resize((840, int(840 * img.height / img.width)), Image.LANCZOS)
                frames.append(img)
        browser.close()
    srv.shutdown()

    pal = [f.quantize(colors=96, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE) for f in frames]
    durations = []
    for _ in SEQUENCE:
        durations += [1500] + [140] * 3 + [1000]
    pal[0].save(OUT / "demo.gif", save_all=True, append_images=pal[1:], duration=durations, loop=0, optimize=True)
    print(f"wrote {OUT / 'demo.png'} and {OUT / 'demo.gif'} ({(OUT / 'demo.gif').stat().st_size // 1024} KiB)")
    if errors:
        print("browser errors:", *errors, sep="\n  ", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
