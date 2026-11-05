# Glasses web surface

Wrapper for the 600x600 Meta Ray-Ban Display web surface.

Canonical references:

- Existing Walkaway glasses app: `/Users/mac/walkaway-hackathon/glasses`
- Meta sample applications: `/Users/mac/meta_glasses_webmcp/examples`,
  especially `examples/market` for the native-feeling embedded UI structure.

Target file shape, copied from the sample-app pattern when implementation
starts:

- `index.html` — `#device`, sticky `#chrome`, `#app`.
- `src/main.ts` — composition and event wiring only.
- `src/state.ts` — route/presentation state.
- `src/render.ts` — renderers and focus placement.
- `src/client.ts` — trusted host-service client.
- `src/tools.ts` — WebMCP tool registration.
- `src/webmcp.ts` — WebMCP transport/types.
- `src/style.css` — 600x600 native-style layout and focus ring.
- `public/agent.md` — long-form agent instructions.

Hard UI constraints are in `docs/glasses-ui-rules.md`. Approval screens use
no more than two primary actions side by side. General navigation grids use
no more than three columns.


## Meta AI debug install redirect

`install-debug.sh` delivers a public HTTPS debug web-app URL to Meta AI on
the connected Pixel through:

```text
fb-viewapp://web_app_deep_link?appName=<url-encoded>&appUrl=<url-encoded>
```

Example:

```sh
./install-debug.sh --name Walkaway --url https://walkaway.secure.build/app/
```

Verified 2026-11-04: the deep link launches and focuses
`com.facebook.stella`. Final in-Meta-AI confirmation and glasses-side app
appearance still require a physical check.
