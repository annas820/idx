#!/usr/bin/env node
// Thin launcher: runs the downloaded idx binary with the same arguments and
// forwards its exit code, so `npx @midoriresearch/idx stocks quote BBCA` and
// `claude mcp add idx -- npx -y @midoriresearch/idx mcp` both work.
"use strict";
const path = require("path");
const fs = require("fs");
const { spawnSync } = require("child_process");

const exe = process.platform === "win32" ? "idx.exe" : "idx";
const bin = path.join(__dirname, exe);
if (!fs.existsSync(bin)) {
  console.error("idx: binary missing; reinstall the package (npm rebuild @midoriresearch/idx) or use install.sh");
  process.exit(1);
}
const r = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
if (r.error) {
  console.error(`idx: ${r.error.message}`);
  process.exit(1);
}
process.exit(r.status === null ? 1 : r.status);
