# Demo backward plan

Status: storyline selected by `/Users/mac/walkaway-hackathon/docs/spec.md`
Phase 6 and section 18.1.

Selected all-demo storyline:

1. One-shot account creation and vault commit.
2. Interactive form navigation and self-correcting validation.
3. Zero-LLM recall and instant sign-in/replay.

Rules:

1. Write the real end-to-end acceptance test first.
2. The test launches the real fixture, real Hermes binary, real host/agent
   channels, real local model endpoint when the storyline calls the model,
   and the real approval surface used on camera.
3. No mocked browser, mocked approval, mocked Android notification, mocked
   credential broker, or fake success state.
4. Build only the shortest vertical slice that makes the acceptance test
   pass repeatedly.
5. Record only after two consecutive clean end-to-end runs.
6. Keep one hour of submission buffer.

First acceptance slice:

- Real localhost signup fixture with mutable account backend.
- Observe the real signup form through Hermes.
- Fill the form through the supervised Hermes control protocol.
- Approve the final submit through the real host boundary.
- Assert directly against the fixture backend that exactly one account was
  created with the submitted non-sensitive fields.
- Assert the password never appears in observe results, proposals, events,
  audit records, or logs.
