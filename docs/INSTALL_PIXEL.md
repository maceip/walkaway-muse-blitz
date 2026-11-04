# Install Walkaway on glasses via the Pixel (Meta AI app, Developer Mode)

Verified device on the bench: Pixel 11 Pro XL with the Meta AI app
(`com.facebook.stella`) and the wearables companion (`com.facebook.aura`).

## 1. Confirm versions and Developer Mode

Per the [Wearables setup guide](https://wearables.developer.meta.com/docs/develop/webapps/setup/):

- Glasses software `v125`+ (Meta AI app → Devices → gear → General → About → Release Version).
- Meta AI app `v272`+ (Meta AI app → Settings → App Info).
- Developer Mode: Meta AI app → Settings → App Info → tap the App version number
  five times → Enable. Developer Mode persists across sessions.
- WebMCP is off by default and enabled per device by Developer Mode or rollout.
  After enabling, restart the glasses and the Meta AI app. Eligibility is decided
  when the Web App launches, so relaunch Walkaway after any toggle.

## 2. Serve the Web App over public HTTPS

Glasses load Web Apps only from public HTTPS URLs. For demo day, run the app
locally and tunnel the port:

```sh
bash scripts/run_all.sh          # Chrome + relay + connector + glasses app on :5173
cloudflared tunnel --url http://127.0.0.1:5173
# note the https://<random>.trycloudflare.com URL
```

The companion relay (`:8790`) and connector (`:8890`) stay on loopback. Only the
glasses page is tunneled. The page calls `/api/*`, which Vite proxies to the relay
on this machine, so browser screenshots never traverse the tunnel host's storage —
frames stream Mac → tunnel → glasses per request.

## 3. Install with the redirect workaround (stable QR, rotating tunnel)

Tunnel URLs change every run; a printed QR cannot. Walkaway solves the mismatch
with a stable redirect page:

1. Host `install/` once on any static HTTPS host (GitHub Pages works).
2. Each session, regenerate the redirect target and republish one small file:

   ```sh
   python3 scripts/make_install.py \
     --app-url https://<random>.trycloudflare.com/ \
     --stable-url https://<you>.github.io/walkaway-hackathon/install/
   ```

3. The generated `install/install-qr.svg` encodes
   `fb-viewapp://web_app_deep_link?appName=Walkaway&appUrl=<stable URL>`.
   That QR never changes. Scan the QR with the glasses, confirm the
   add-to-glasses screen (Walkaway shows as unverified until reviewed — expected).
4. Alternative from a page already open on glasses: the in-app
   **Add to glasses** button calls `navigator.install()`.

## 4. Smoke test on hardware

1. Open Walkaway on the glasses. Header reads `idle`.
2. Start a task from the desktop browser (same URL) or by voice.
3. Propose a step. The lens shows the screenshot and one action line.
4. Say "approve" (WebMCP `walkaway_approve_step`) or focus **Approve & act**.
   Exactly one browser step executes on the Mac.
5. Reject the next proposal. Confirm the Mac browser does not move.
