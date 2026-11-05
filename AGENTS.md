# Agent coordination rules — read before any work

This project has multiple agents. Duplicated implementations and UI drift are
release-blocking failures.

Before reading code for a new task, editing, starting a server, or creating a
file, read these in order:

1. `docs/STATUS.md` — the only shared status and ownership board.
2. `docs/source-map.md` — the only canonical location map.
3. `docs/TEST_REGISTRY.md` — what each real-system test actually proves.
4. The governing technical spec named in `docs/STATUS.md`.

Rules:

- One implementation per capability. If a component already exists, extend
  the canonical component or stop and resolve the conflict in `docs/STATUS.md`.
- Claim a component in `docs/STATUS.md` before editing the component.
- Do not start a server until `docs/STATUS.md` and the live process table show
  that the port and role are unused.
- Do not create a new UI surface, service, protocol, client, or fixture
  without adding the surface or service to `docs/STATUS.md` and
  `docs/source-map.md` in the same change.
- A test is not evidence unless `docs/TEST_REGISTRY.md` names the exact
  command, real components, assertion, and last observed result.
- No mocked browser, approval, Android notification, credential broker, model,
  or success state in demo-path tests.
- At every milestone, update `docs/STATUS.md` with: current owner, exact
  verification command, observed result, next action, and known blocker.
- If another agent owns the component or a live process conflicts, stop.
  Do not kill, replace, or work around that component without recording the
  resolution.
