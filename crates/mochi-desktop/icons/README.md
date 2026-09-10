# Application icons

`source.svg` is the original. Everything else is generated from it:

```sh
# Rasterise the SVG at 1024×1024 by any means (the repo's Chromium works), then:
npx tauri icon path/to/icon-1024.png --output crates/mochi-desktop/icons
```

`tauri icon` also writes Android and iOS icon sets and Windows Store logos.
Mochi is a desktop application for Windows and macOS (doc/requirements.md §2.2)
and bundles only `nsis`, `msi`, `dmg` and `app`, so those are deleted rather
than carried.
