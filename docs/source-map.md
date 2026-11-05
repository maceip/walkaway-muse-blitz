# Source map

| Component | Wrapper | Canonical working source today | Migration rule |
|---|---|---|---|
| Hermes browser | `apps/hermes-browser/` | `/Users/mac/hermes-blitz-browser` | Migrate only the frozen demo build |
| Android bridge | `apps/android-notification-bridge/` | Generated empty Activity in this repo | Build behavior after demo selection |
| Glasses web | `apps/glasses-web/` | `/Users/mac/walkaway-hackathon/glasses` | Rebuild against selected demo and sample-app patterns |
| Meta sample references | external | `/Users/mac/meta_glasses_webmcp/examples` | Reference only; do not vendor blindly |
| Phone approval | `apps/phone-approval-web/` | Existing Walkaway responsive approval surface | Split only if demo requires separate phone UX |
| Muse connector | `services/muse-connector/` | `/Users/mac/walkaway-hackathon/connector` | Keep deployed service stable during review |
| Chrome relay | `services/walkaway-relay/` | `/Users/mac/walkaway-hackathon/companion` | Keep separate from Hermes execution |
| Hermes host bridge | `services/hermes-host/` | `/Users/mac/walkaway-monorepo/services/hermes-host/decision_bridge.py` (+ `message_bus.py` on 19099) | Canonical; bridge HTTP API on 19101 (`/health`, `/pending`, `/decisions`, `POST /cards`) |
| Agent driver | `services/agent-driver/` | `/Users/mac/walkaway-monorepo/services/agent-driver/hermes_agent_driver.py` | Canonical agent-channel driver (real local Qwen3-4B + zero-LLM recall) |
| Demo fixture | `services/demo-fixtures/` | `/Users/mac/walkaway-monorepo/services/demo-fixtures/signup_fixture.py` | Canonical SQLite-backed signup, CFP validation, and sign-in fixture |
| Marketing surface | `apps/marketing-site/` | `/Users/mac/walkaway-monorepo/apps/marketing-site/` | Static no-build site (one `index.html` + `assets/`). Single implementation for the Walkaway × Muse Blitz project page and its marketing asset kit — extend this directory rather than adding a second marketing surface |
