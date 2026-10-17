#!/usr/bin/env node
// postinstall: fetch the binary now so the first run is instant. When npm
// skips install scripts (npm 12 default, --ignore-scripts, some CI images)
// this never runs and bin/idx.js downloads on first use instead, so a
// failure here is a warning, not an install error.
"use strict";
const binary = require("./lib/binary");

if (binary.VERSION === "0.0.0") {
  console.log("idx: development package version; skipping binary download");
  process.exit(0);
}
if (process.env.IDX_SKIP_DOWNLOAD === "1") {
  console.log("idx: IDX_SKIP_DOWNLOAD=1; the binary will be fetched on first run");
  process.exit(0);
}
binary.download(console.log).catch((e) => {
  console.warn(`idx: could not download the binary now (${e.message}); it will be fetched on first run.`);
  console.warn(`idx: or install it manually: curl -fsSL https://raw.githubusercontent.com/${binary.REPO}/main/install.sh | sh`);
  process.exit(0);
});
