#!/usr/bin/env node
// research-agent plugin bootstrap — runs on SessionStart via
// .claude-plugin/hooks.json. Installs the `research` binary if missing,
// cross-platform. Always non-fatal: a failure must never break the Claude Code
// session (it just logs + exits 0).
//
// Uses only Node.js built-ins — no npm install. Claude Code is built on Node,
// so `node` is always on the PATH it uses to launch hook commands.
//
// research-agent ships binaries via GitHub Releases (and a Homebrew tap), but
// this hook deliberately installs from the release channel so it works without
// brew: it downloads the install.sh / install.ps1 from the latest release.

"use strict";

const { spawnSync } = require("child_process");
const fs = require("fs");
const { createWriteStream, chmodSync, readFileSync } = fs;
const { join } = require("path");
const { tmpdir } = require("os");
const https = require("https");
const os = require("os");

// The hook runs at session start with async:false — never let a stalled
// download or a wedged installer block the session for long.
const CONNECT_TIMEOUT_MS = 10_000;
const DOWNLOAD_TIMEOUT_MS = 60_000;
const INSTALL_TIMEOUT_MS = 300_000;

const REPO = "epicsagas/research-agent";
const BINARY = "research";
const INSTALLER_SH = `https://github.com/${REPO}/releases/latest/download/install.sh`;
const INSTALLER_PS1 = `https://github.com/${REPO}/releases/latest/download/install.ps1`;

function log(msg) {
  process.stderr.write(`[research plugin] ${msg}\n`);
}

/** True if `research` is installed and runnable. */
function hasBinary() {
  const r = spawnSync(BINARY, ["--help"], { stdio: "pipe", shell: false });
  return r.status === 0;
}

/** Semver string from `research --version`, or null. */
function getBinaryVersion() {
  try {
    const r = spawnSync(BINARY, ["--version"], { stdio: "pipe", shell: false });
    if (r.status === 0 || (r.stdout && r.stdout.length)) {
      const out = (r.stdout ? r.stdout.toString() : "") + (r.stderr ? r.stderr.toString() : "");
      const m = out.match(/(\d+\.\d+\.\d+)/);
      return m ? m[1] : null;
    }
  } catch (_) {}
  return null;
}

/** Plugin manifest version (source of truth for "is the binary behind?"). */
function getPluginVersion() {
  const root = process.env.CLAUDE_PLUGIN_ROOT || process.env.GROK_PLUGIN_ROOT || "";
  // grok trees may not carry .claude-plugin/; fall back to its own manifest.
  for (const rel of [
    join(".claude-plugin", "plugin.json"),
    join(".grok-plugin", "plugin.json"),
  ]) {
    try {
      return JSON.parse(readFileSync(join(root, rel), "utf8")).version || null;
    } catch (_) {}
  }
  return null;
}

function semverGt(a, b) {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    if (pa[i] > pb[i]) return true;
    if (pa[i] < pb[i]) return false;
  }
  return false;
}

function downloadFile(url, dest) {
  return new Promise((resolve, reject) => {
    const file = createWriteStream(dest);
    const follow = (u, depth) => {
      if (depth > 5) {
        reject(new Error(`too many redirects for ${url}`));
        return;
      }
      const req = https.get(u, (res) => {
        if (res.statusCode === 301 || res.statusCode === 302) {
          const location = res.headers.location;
          res.resume();
          if (!location) {
            reject(new Error(`redirect without Location for ${u}`));
            return;
          }
          follow(location, depth + 1);
          return;
        }
        if (res.statusCode !== 200) {
          reject(new Error(`HTTP ${res.statusCode} for ${u}`));
          return;
        }
        // Overall transfer timeout: a stalled body must not hang the session.
        res.setTimeout(DOWNLOAD_TIMEOUT_MS, () => {
          req.destroy(new Error(`download timed out for ${u}`));
        });
        res.pipe(file);
        file.on("finish", () => file.close(resolve));
      });
      // Connect timeout: no response at all must not hang the session.
      req.setTimeout(CONNECT_TIMEOUT_MS, () => {
        req.destroy(new Error(`connect timed out for ${u}`));
      });
      req.on("error", (e) => {
        file.close(() => reject(e));
      });
      file.on("error", reject);
    };
    follow(url, 0);
  });
}

/** Download + run the platform installer (install.sh / install.ps1). */
async function install() {
  // Private temp dir per run: concurrent sessions must not share files.
  const dir = fs.mkdtempSync(join(tmpdir(), "research-install-"));
  try {
    if (os.platform() === "win32") {
      const tmp = join(dir, "installer.ps1");
      log("Downloading Windows installer...");
      await downloadFile(INSTALLER_PS1, tmp);
      const r = spawnSync(
        "powershell",
        ["-ExecutionPolicy", "Bypass", "-File", tmp],
        { stdio: "inherit", timeout: INSTALL_TIMEOUT_MS }
      );
      if (r.status !== 0) throw new Error("PowerShell installer failed");
    } else {
      const tmp = join(dir, "installer.sh");
      log("Downloading installer...");
      await downloadFile(INSTALLER_SH, tmp);
      chmodSync(tmp, 0o755);
      const r = spawnSync("sh", [tmp], { stdio: "inherit", timeout: INSTALL_TIMEOUT_MS });
      if (r.status !== 0) throw new Error("Shell installer failed");
    }
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

async function main() {
  // 1. Binary missing — fresh install.
  if (!hasBinary()) {
    log(`${BINARY} not found — installing...`);
    try {
      await install();
    } catch (e) {
      log(`Install failed: ${e.message}`);
      log(`Install manually: https://github.com/${REPO}#installation`);
      process.exit(0); // non-fatal
    }
    return;
  }

  // 2. Binary present — upgrade if the plugin ships a newer version.
  const pluginVersion = getPluginVersion();
  if (pluginVersion) {
    const binaryVersion = getBinaryVersion();
    if (binaryVersion && semverGt(pluginVersion, binaryVersion)) {
      log(`Updating ${BINARY} ${binaryVersion} → ${pluginVersion}...`);
      try {
        await install();
        const now = getBinaryVersion();
        if (now) log(`Updated to ${now}`);
      } catch (e) {
        log(`Update failed: ${e.message} — continuing with ${binaryVersion}`);
        // non-fatal; old binary still works
      }
    }
  }
}

main().catch((e) => {
  log(`Unexpected error: ${e.message}`);
  process.exit(0); // non-fatal — never break the session
});
