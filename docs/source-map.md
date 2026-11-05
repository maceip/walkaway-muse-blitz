# Source map

| Component | Wrapper | Canonical working source today | Migration rule |
|---|---|---|---|
| Hermes browser | `apps/hermes-browser/` | `/Users/mac/hermes-blitz-browser` | Migrate only the frozen demo build |
| Android bridge | `apps/android-notification-bridge/` | Generated empty Activity in this repo | Build behavior after demo selection |
| Glasses web | DELETED 2026-11-05 (vapor stub) | `/Users/mac/walkaway-hackathon/glasses` (separate older app, not in this repo) | Native DAT is the glasses surface; resurrect only as real code on Ryan decision |
| Meta sample references | external | `/Users/mac/meta_glasses_webmcp/examples` | Reference only; do not vendor blindly |
| Phone approval | DELETED 2026-11-05 (README-only stub) | — | Phone is state-only (Android bridge); no separate approval surface |
| Muse connector | `services/muse-connector/` | `/Users/mac/walkaway-hackathon/connector` | Keep deployed service stable during review |
| Chrome relay | `services/walkaway-relay/` | `/Users/mac/walkaway-hackathon/companion` | Keep separate from Hermes execution |
| Hermes host bridge | `services/hermes-host/` | `/Users/mac/walkaway-monorepo/services/hermes-host/decision_bridge.py` (+ `message_bus.py` on 19099) | Canonical; bridge HTTP API on 19101 (`/health`, `/pending`, `/decisions`, `POST /cards`) |
| Agent driver | `services/agent-driver/` | `/Users/mac/walkaway-monorepo/services/agent-driver/hermes_agent_driver.py` | Canonical agent-channel driver (real local Qwen3-4B + zero-LLM recall) |
| Demo fixture | `services/demo-fixtures/` | `/Users/mac/walkaway-monorepo/services/demo-fixtures/signup_fixture.py` | Canonical SQLite-backed signup, CFP validation, and sign-in fixture |
| Marketing surface | `apps/marketing-site/` | `/Users/mac/walkaway-monorepo/apps/marketing-site/` | Static no-build site (one `index.html` + `assets/`). Single implementation for the Walkaway project page and its marketing asset kit — extend this directory rather than adding a second marketing surface |
| Marketing creatives | `scripts/build-posters.py` | `/Users/mac/walkaway-monorepo/scripts/build-posters.py` | Single generator for every poster and paid placement. Writes `apps/marketing-site/assets/posters/*.svg` (10 × 1920×1080) and `apps/marketing-site/assets/banners/*.svg` (6 × true ad ratios) — fonts embedded as base64, no gradients, no scripts. Regenerate with `python3 scripts/build-posters.py`; never hand-edit the generated SVGs |
| Hero artwork | `scripts/build-hero-art.py` | `/Users/mac/walkaway-monorepo/scripts/build-hero-art.py` | Single generator for the composed campaign artwork. Composites the real Hermes browser crop (out of `apps/marketing-site/assets/download/hero-winged-chips.png`) with the transparent mascot and chip cutouts and writes `hero-muse.png`, `hero-muse-flight.png`, `og-walkaway.png` and `square-muse.png` into `apps/marketing-site/assets/download/`. Regenerate with `/tmp/fontenv/bin/python scripts/build-hero-art.py`; never hand-edit the generated PNGs |
