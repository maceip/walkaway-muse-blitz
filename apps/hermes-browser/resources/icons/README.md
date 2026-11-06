# Browser app icon

These six 1024px PNGs were supplied by the owner in the browser icon request.
They are preserved without modification from the corresponding `Icon-iOS-*`
exports:

- `default.png`: blue Default artwork, primary macOS Finder/Dock app icon.
- `dark.png`: yellow/black Dark artwork.
- `tinted-light.png`, `tinted-dark.png`: teal TintedLight/TintedDark artwork.
- `clear-light.png`, `clear-dark.png`: grey ClearLight/ClearDark artwork.

The appearance variants are source assets, not automatic macOS appearance
switching. The bundle currently uses the single primary `AppIcon.icns` referenced
by `CFBundleIconFile`; no unsupported variant-switching claim is made.

Generate and verify a complete 16px–1024px macOS icon with:

```sh
bash apps/hermes-browser/scripts/build-app-icon.sh /tmp/Walkaway-AppIcon.icns
```

The builder uses a proper `.iconset` subdirectory, fails on conversion errors,
and round-trips the 1024px representation against `default.png` byte-for-byte.
The DMG builder installs this resource before signing and checks the same bytes
and Info.plist reference from the mounted disk image. Generated ICNS/iconsets
remain build artifacts, not independent hand-edited source assets.
