# Application icons

`source.svg` is the original. Everything else is generated from it:

Rasterise the SVG at 1024×1024 by any means (the repo's Chromium works), then
scale it down to the sizes here with any image tool.

`128x128@2x.png` is the one that matters: it is compiled into the executable
and becomes the window's icon at runtime. The rest are kept for the platform
icon formats — `icon.ico` for a Windows resource, `icon.icns` for a macOS
bundle — neither of which the single-file build uses yet.
