# Demo fixtures

Real local services for the selected demo belong here.

Rules:

- A fixture owns real mutable backend state; the browser action must change
  that state through the actual page controls.
- No mocked browser, mocked approval, mocked notification, or hard-coded
  success response.
- Fixture state must be independently observable by the end-to-end test.
