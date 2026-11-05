# Agent driver

Wrapper for the agent-channel client used by the selected demo.

Boundary:

- Connects to Hermes using only the agent token.
- Observes, proposes, and signals task completion.
- Cannot open sessions, decide proposals, take control, release control, or
  stop as a host authority, except through product-approved agent messages.
- Model/provider choice is locked only after the demo storyline is selected
  and the real provider path is verified.
