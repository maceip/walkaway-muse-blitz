# Phone approval web surface

Wrapper for the phone-sized approval and status surface.

Boundary:

- Shows the same pending proposal, screenshot/frame, session state, and
  audit receipt as the glasses surface.
- Sends human decisions only to the trusted host service.
- Uses large, thumb-safe controls; approval actions remain at most two
  side by side.
- Does not contain browser execution logic.
