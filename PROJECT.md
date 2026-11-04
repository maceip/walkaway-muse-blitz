# Secure Browser with Approval — project control

Single source of truth. If work is not on this page, work is not in scope.

## Locked decisions (2026-11-03, owner-approved)

- Product: **Secure Browser with Approval**. Muse delegates browser tasks via
  the Muse Connector to a typed browser agent (Ultrafast contract) on the
  user's machine. A human approves each consequential step on an approval page.
- Connector listing name: `Secure Browser with Approval`. App/brand: Walkaway.
- Approval surfaces: phone browser (primary), Meta Ray-Ban Display (bonus).
  Glasses are never load-bearing in pitch, demo, or judging claims.
- Capability rule: the connector has no approve tool. Execution requires an
  approval recorded by the approval page against the relay. Reject deletes the
  pending decision relay-side.
- Decision model contract: typed operation + observed target only; unoffered
  targets rejected; decisions consumed once; stale pages invalidate decisions.
- Model labeling rule: the UI and every document print the model actually
  running. Muse Spark path activates only when `MODEL_API_KEY` is present.
- Deadline: hackathon submission Nov 4, 21:00 ET. Connector form submitted by
  owner from `submission/SUBMISSION.md` to start Meta review loop.

## No-go list

- No fake, scripted, or rendered evidence in any UI, page, or document.
- No dead controls. Every rendered control calls real backed state.
- No new surfaces, scenarios, or models beyond the checklist below.
- No glasses-required claims. No market-size claims. No speed numbers that
  were not measured on the current stack in this repo.

## Remaining checklist

- [x] Approval page responsive for phone browsers
- [x] Connector returns `approval_url`; copy is surface-neutral
- [x] README + VIDEO_SCRIPT recentered on Secure Browser
- [x] Redeploy + hosted loop verified
- [ ] Owner: paste connector form, submit (starts Meta review loop)
- [ ] Owner: add `MODEL_API_KEY` (dev.meta.ai) to `~/walkaway-hackathon/.env`
- [ ] Verify Muse Spark chooser live, measure per-step latency, record numbers
- [ ] Google Flights scenario verified end-to-end on hosted stack
- [ ] Record 3-minute video per VIDEO_SCRIPT.md
- [ ] Publish GitHub repo open-source (hackathon requirement)
- [ ] Android driver app: build + install on Pixel (device must be on ADB)
- [ ] Hackathon submission form submitted before Nov 4, 21:00 ET

## Live endpoints

- Connector: https://muse.secure.build/mcp
- Approval page: https://walkaway.secure.build/app/ (relay token via `?k=`)
- Product/legal: https://walkaway.secure.build/ · /privacy · /terms · /support · /docs
