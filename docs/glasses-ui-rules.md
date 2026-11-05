# Glasses UI rules — 600x600

These rules follow Meta's sample applications, especially `examples/market`,
and the dexterity limits of sequential focus/neural-band input.

- Design and test inside a fixed 600x600 `#device` surface.
- No pointer assumptions. Every interactive element must be reachable by
  sequential focus.
- Approval screens: at most **two** actions side by side.
- General grids: at most **three** columns.
- One primary decision per viewport. Put secondary information below the
  decision or on the next focus step.
- Focus the safest action first when a screen opens.
- Use a visible 2px focus outline with offset. Do not rely on hover.
- Use large controls and generous spacing; avoid dense icon rows and tiny
  close buttons.
- Keep the header sticky and opaque. Do not use backdrop blur.
- Avoid transform/opacity compositor animations on glasses; follow the
  sample apps' custom-property animation pattern when animation is needed.
- Text must remain readable without zoom. Do not place critical warnings in
  muted microcopy.
- Destructive approval requires the exact action and target on screen.
- No dead controls. Every rendered control must call the real host boundary
  or be removed.
