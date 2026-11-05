# Demo architecture boundaries

```text
Muse / selected model
        |
        | agent role
        v
services/agent-driver ---- agent token ----> apps/hermes-browser
                                                    ^
Human surfaces                                      | host token
- apps/glasses-web ----------------> services/hermes-host
- apps/phone-approval-web ---------> services/hermes-host
- apps/desktop-supervision --------> services/hermes-host
                                                    |
apps/android-notification-bridge <--- action-required events
        |
        v
Android / Meta AI notification pathway toward glasses

services/muse-connector = external Muse MCP entry point
services/walkaway-relay = separate existing Chrome execution path
services/demo-fixtures  = real local pages/services the browser operates
```

Authority rules:

- Hermes is the only browser executor in the Hermes demo path.
- The agent can observe and propose; the agent cannot approve.
- Human decisions enter only through the host boundary.
- Android transports attention/events; Android does not hold browser
  execution authority.
- The glasses web surface renders the exact pending proposal and returns a
  human decision; the glasses web surface does not execute browser actions.
