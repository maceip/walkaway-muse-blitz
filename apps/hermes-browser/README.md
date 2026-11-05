# Hermes browser

Native supervised local browser wrapper.

Canonical working source: `/Users/mac/hermes-blitz-browser`

Boundary:

- Owns canonical tabs, page instances, local browsing, observation,
  proposals, decisions, executions, screenshots, and audit events.
- Exposes the authenticated Unix-socket NDJSON host/agent control API.
- Does not own Muse account credentials, Android notifications, or the
  glasses web UI.

Migration trigger: copy or subtree-import only when the selected demo is
ready to freeze its exact browser build.
