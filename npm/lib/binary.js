// Locates or downloads the idx release binary for this platform.
//
// Used by install.js (postinstall) and by bin/idx.js on first run, because
// npm 12 no longer runs dependency install scripts by default: with scripts
// blocked the package installs without a binary, and the launcher fetches
// it on first use instead. Either path verifies SHA-256 against the
// release's SHA256SUMS before writing anything executable.
"use strict";
const fs = require("fs");
const os = require("os");
const path = require("path");
const https = require("https");
const crypto = require("crypto");
const zlib = require("zlib");
const { execFileSync } = require("child_process");

const pkg = require("../package.json");
const REPO = process.env.IDX_REPO || "fzn0x/idx";
const VERSION = process.env.IDX_VERSION || pkg.version;
const BIN_DIR = path.join(__dirname, "..", "bin");
const EXE = os.platform() === "win32" ? "idx.exe" : "idx";

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

/** Path of the binary if it is already installed, else null. */
function installed() {
  const p = path.join(BIN_DIR, EXE);
  return fs.existsSync(p) ? p : null;
}

/**
 * Downloads and verifies the binary for `VERSION`. `log` receives progress
 * lines (stderr for the launcher, stdout for postinstall). Resolves to the
 * binary path.
 */
async function download(log = () => {}) {
  if (VERSION === "0.0.0") throw new Error("development package version 0.0.0 has no release to download");
  const { triple, ext } = target();
  const name = `idx-${VERSION}-${triple}`;
  const asset = `${name}.${ext}`;
  const base = `https://github.com/${REPO}/releases/download/v${VERSION}`;
  log(`idx: downloading ${asset}`);
  const [archive, sums] = await Promise.all([get(`${base}/${asset}`), get(`${base}/SHA256SUMS`)]);
  const line = sums.toString().split("\n").find((l) => l.trim().endsWith(` ${asset}`) || l.trim().endsWith(`*${asset}`));
  if (!line) throw new Error(`${asset} missing from SHA256SUMS`);
  const expected = line.trim().split(/\s+/)[0];
  const actual = crypto.createHash("sha256").update(archive).digest("hex");
  if (expected !== actual) throw new Error(`checksum mismatch for ${asset}: expected ${expected}, got ${actual}`);

  fs.mkdirSync(BIN_DIR, { recursive: true });
  const dest = path.join(BIN_DIR, EXE);
  const tmp = `${dest}.${process.pid}.tmp`;
  if (ext === "tar.gz") {
    fs.writeFileSync(tmp, untar(zlib.gunzipSync(archive), EXE), { mode: 0o755 });
  } else {
    // Windows: hand the zip to PowerShell rather than shipping a zip parser.
    const zipPath = path.join(os.tmpdir(), asset);
    const outDir = path.join(os.tmpdir(), `${name}-${process.pid}`);
    fs.writeFileSync(zipPath, archive);
    execFileSync("powershell", [
      "-NoProfile",
      "-Command",
      `Expand-Archive -Force -LiteralPath '${zipPath}' -DestinationPath '${outDir}'`,
    ]);
    fs.copyFileSync(path.join(outDir, name, EXE), tmp);
    fs.rmSync(outDir, { recursive: true, force: true });
    fs.rmSync(zipPath, { force: true });
  }
  fs.renameSync(tmp, dest); // atomic: a concurrent first run never sees a half-written file
  log(`idx: installed ${dest} (sha256 verified)`);
  return dest;
}

/** Installed path, downloading first when needed. */
async function ensure(log) {
  return installed() || download(log);
}

module.exports = { installed, download, ensure, target, REPO, VERSION, BIN_DIR, EXE };
