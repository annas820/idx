#!/usr/bin/env node
// Downloads the idx release matching this package's version for the current
// platform into ./bin/, verifying SHA-256 against the release's SHA256SUMS.
"use strict";
const fs = require("fs");
const os = require("os");
const path = require("path");
const https = require("https");
const crypto = require("crypto");
const zlib = require("zlib");
const { execFileSync } = require("child_process");

const pkg = require("./package.json");
const REPO = process.env.IDX_REPO || "fzn0x/idx";
const VERSION = process.env.IDX_VERSION || pkg.version;
const BIN_DIR = path.join(__dirname, "bin");

function target() {
  const plat = os.platform();
  const arch = os.arch();
  const a = arch === "x64" ? "x86_64" : arch === "arm64" ? "aarch64" : null;
  if (!a) throw new Error(`unsupported architecture ${arch}`);
  if (plat === "linux") return { triple: `${a}-unknown-linux-musl`, ext: "tar.gz" };
  if (plat === "darwin") return { triple: `${a}-apple-darwin`, ext: "tar.gz" };
  if (plat === "win32" && a === "x86_64") return { triple: "x86_64-pc-windows-msvc", ext: "zip" };
  throw new Error(`unsupported platform ${plat}/${arch}`);
}

function get(url, redirects = 0) {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: { "User-Agent": "idx-npm-installer" } }, (res) => {
        if ([301, 302, 303, 307, 308].includes(res.statusCode) && res.headers.location && redirects < 5) {
          res.resume();
          return resolve(get(res.headers.location, redirects + 1));
        }
        if (res.statusCode !== 200) {
          res.resume();
          return reject(new Error(`${url}: HTTP ${res.statusCode}`));
        }
        const chunks = [];
        res.on("data", (c) => chunks.push(c));
        res.on("end", () => resolve(Buffer.concat(chunks)));
        res.on("error", reject);
      })
      .on("error", reject);
  });
}

// Minimal tar reader: enough for the release archives (ustar, plain files).
function untar(buf, wantedName) {
  let off = 0;
  while (off + 512 <= buf.length) {
    const header = buf.subarray(off, off + 512);
    if (header.every((b) => b === 0)) break;
    const name = header.subarray(0, 100).toString().replace(/\0.*$/, "");
    const size = parseInt(header.subarray(124, 136).toString().replace(/\0.*$/, "").trim(), 8) || 0;
    const prefix = header.subarray(345, 500).toString().replace(/\0.*$/, "");
    const full = prefix ? `${prefix}/${name}` : name;
    off += 512;
    if (path.posix.basename(full) === wantedName && header[156] !== 53) {
      return buf.subarray(off, off + size);
    }
    off += Math.ceil(size / 512) * 512;
  }
  throw new Error(`${wantedName} not found in archive`);
}

async function main() {
  if (VERSION === "0.0.0") {
    console.log("idx: development package version; skipping binary download");
    return;
  }
  const { triple, ext } = target();
  const exe = os.platform() === "win32" ? "idx.exe" : "idx";
  const name = `idx-${VERSION}-${triple}`;
  const asset = `${name}.${ext}`;
  const base = `https://github.com/${REPO}/releases/download/v${VERSION}`;
  console.log(`idx: downloading ${asset}`);
  const [archive, sums] = await Promise.all([get(`${base}/${asset}`), get(`${base}/SHA256SUMS`)]);
  const line = sums.toString().split("\n").find((l) => l.trim().endsWith(` ${asset}`) || l.trim().endsWith(`*${asset}`));
  if (!line) throw new Error(`${asset} missing from SHA256SUMS`);
  const expected = line.trim().split(/\s+/)[0];
  const actual = crypto.createHash("sha256").update(archive).digest("hex");
  if (expected !== actual) throw new Error(`checksum mismatch for ${asset}: expected ${expected}, got ${actual}`);

  fs.mkdirSync(BIN_DIR, { recursive: true });
  const dest = path.join(BIN_DIR, exe);
  if (ext === "tar.gz") {
    const tar = zlib.gunzipSync(archive);
    fs.writeFileSync(dest, untar(tar, exe), { mode: 0o755 });
  } else {
    // Windows: hand the zip to PowerShell rather than shipping a zip parser.
    const tmp = path.join(os.tmpdir(), asset);
    fs.writeFileSync(tmp, archive);
    execFileSync("powershell", [
      "-NoProfile",
      "-Command",
      `Expand-Archive -Force -LiteralPath '${tmp}' -DestinationPath '${BIN_DIR}'`,
    ]);
    fs.renameSync(path.join(BIN_DIR, name, exe), dest);
    fs.rmSync(path.join(BIN_DIR, name), { recursive: true, force: true });
    fs.rmSync(tmp, { force: true });
  }
  console.log(`idx: installed ${dest} (sha256 verified)`);
}

main().catch((e) => {
  console.error(`idx: install failed: ${e.message}`);
  console.error(`idx: you can install manually: curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | sh`);
  process.exit(1);
});
