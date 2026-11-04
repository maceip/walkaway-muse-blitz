"""Generate the Walkaway glasses install page + QR code.

The redirect workaround: tunnel URLs rotate, QR codes do not. The QR encodes
the Meta install deep link
  fb-viewapp://web_app_deep_link?appName=Walkaway&appUrl=<STABLE_HTTPS_URL>
where STABLE_HTTPS_URL hosts install/index.html. That page immediately
redirects to the current app URL (tunnel or production). Regenerate and
republish only install/index.html when the app URL changes; the printed QR
keeps working.

Usage:
  python3 scripts/make_install.py --app-url https://<tunnel>.trycloudflare.com/ \
      --stable-url https://<you>.github.io/walkaway/install/
Outputs: install/index.html, install/install-qr.svg (needs segno or qrcode;
install with: uvx --from segno segno --help, or pip install segno).
"""

import argparse
import urllib.parse
from pathlib import Path

REDIRECT = """<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="application-name" content="Walkaway" />
    <meta name="description" content="Walkaway: approve your browser agent's steps from your glasses" />
    <meta name="mrbd-web-app-capable" content="yes" />
    <title>Walkaway</title>
  </head>
  <body>
    <p>Opening Walkaway…</p>
    <script>location.replace({app_url!r});</script>
    <p><a href="{app_url}">Open Walkaway</a></p>
  </body>
</html>
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--app-url", required=True, help="Current HTTPS URL of the glasses Web App")
    ap.add_argument("--stable-url", required=True, help="Public HTTPS URL where install/index.html will be hosted")
    args = ap.parse_args()
    if not args.app_url.startswith("https://") or not args.stable_url.startswith("https://"):
        raise SystemExit("Both URLs must be https:// (the glasses QR parser rejects http://)")
    out = Path(__file__).parent.parent / "install"
    out.mkdir(exist_ok=True)
    (out / "index.html").write_text(REDIRECT.format(app_url=args.app_url))
    deep_link = "fb-viewapp://web_app_deep_link?" + urllib.parse.urlencode(
        {"appName": "Walkaway", "appUrl": args.stable_url}
    )
    try:
        import segno
    except ImportError:
        print(f"install/index.html written.\nDeep link: {deep_link}\n"
              "QR not generated: pip install segno, then re-run.")
        return
    segno.make(deep_link, error="m").save(str(out / "install-qr.svg"), scale=8)
    print(f"Wrote install/index.html and install/install-qr.svg\nDeep link: {deep_link}")


if __name__ == "__main__":
    main()
