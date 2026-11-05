# Desktop supervision surface

Boundary for desktop-side human supervision.

The production Hermes build currently renders its supervision bar inside the
browser window. This folder exists for a future standalone desktop host
client only if the selected demo requires separating that surface from the
browser process. It must use the host role, never the agent role.
