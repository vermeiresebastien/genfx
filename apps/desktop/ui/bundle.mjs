// Rebuilds apps/desktop/ui/app.bundle.js from the ES-module entry (main.js)
// and rewrites the version query on the script URL with a content hash, so a
// new bundle is never served from a stale WebView2 cache.
//
// Usage: node bundle.mjs
// Imported by check-bundle.mjs for the pure parts (see isEntry at the bottom).
//
// Two rules this file exists to obey, and the one it broke at 63bc321:
//
//   1. A run must PROVE it versioned the tag. The old pattern required `?v=`
//      to be present already, so a plain `src="app.bundle.js"` in index.html was
//      left untouched while the script still printed a hash and exited 0 — D036
//      again: a step that prints success and means nothing. It also never asked
//      whether anything had matched at all, and its regex was non-global, so
//      "exactly one" could not have been measured even in principle.
//   2. Writing identical bytes back is not free. index.html is a
//      `cargo:rerun-if-changed` input of the very build script that runs this
//      file, so an unconditional write made every `cargo build -p flux-desktop`
//      re-run it (measured 2026-11-16: index.html's mtime equalled each of two
//      consecutive builds with nothing else touched). The write is therefore
//      guarded by a content comparison, which is what gives the rerun gate a
//      fresh baseline to compare against at all.
//
// What is deliberately NOT done here: the hashed-file NAME form
// (`app.bundle.<hash>.js`) is accepted on the left and normalised away on the
// right. The artifact on disk is apps/desktop/ui/app.bundle.js and Tauri embeds
// that path, so emitting a hashed filename would point the tag at a file nobody
// writes. Only the query carries the version.

import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const bundlePath = join(here, "app.bundle.js");
const indexPath = join(here, "index.html");

// The app.bundle.js script src attribute, whether or not it carries a version
// query and whether or not it carries the hashed-file form of the name.
// Global on purpose: countScriptTags IS the assertion, and a non-global regex
// would report 1 for any input containing at least one tag, which makes
// "exactly one" true by construction — the same vacuity as asserting a
// substring that both the success and the refusal path print.
const SCRIPT_SRC = /src="app\.bundle(?:\.[0-9a-f]{12})?\.js(?:\?v=[^"]*)?"/g;

/** The version token: 12 hex of the bundle's sha256. Content-addressed, so a
 *  changed bundle provably changes the tag and an unchanged one provably does
 *  not. */
export function bundleUrlToken(bytes) {
  return createHash("sha256").update(bytes).digest("hex").slice(0, 12);
}

/** How many app.bundle.js script src attributes this HTML carries. */
export function countScriptTags(html) {
  return (html.match(SCRIPT_SRC) ?? []).length;
}

/**
 * Point every app.bundle.js script tag at `token`.
 *
 * Returns the rewritten HTML plus how many tags it matched, so the caller can
 * insist on exactly one instead of assuming it. `changed` is what decides
 * whether index.html gets written.
 */
export function versionScriptTag(html, token) {
  const tags = html.match(SCRIPT_SRC) ?? [];
  const out = html.replace(SCRIPT_SRC, `src="app.bundle.js?v=${token}"`);
  return { html: out, matched: tags.length, tags, changed: out !== html };
}

function refuse(count, tags) {
  console.error(
    `index.html must carry exactly one app.bundle.js script tag; found ${count} ` +
      `[${tags.join(", ") || "no match"}]. ` +
      (count === 0
        ? "Nothing was versioned: the next changed bundle would be served from " +
          "WebView2's cache, which is the failure this script exists to prevent."
        : "One token cannot version two tags, and guessing which one you meant " +
          "is not this script's call."),
  );
  process.exit(1);
}

async function runEsbuild() {
  // Imported dynamically so that reading the pure functions above from another
  // script does not require esbuild to be installed. A missing esbuild still
  // ends the run: the import rejects, this reports why, and
  // apps/desktop/src-tauri/build.rs (I5 item 1) turns the nonzero exit into a
  // build failure instead of embedding the old bundle.
  let build;
  try {
    ({ build } = await import("esbuild"));
  } catch (e) {
    console.error(
      `esbuild could not be loaded from apps/desktop/ui (${e.message}). Run ` +
        `"npm ci" in apps/desktop/ui; this script does not fall back to the ` +
        "committed bundle.",
    );
    process.exit(1);
  }
  const result = await build({
    entryPoints: [join(here, "main.js")],
    bundle: true,
    format: "iife",
    target: "es2020",
    outfile: bundlePath,
    logLevel: "silent",
    platform: "browser",
  });
  if (result.errors.length) {
    console.error("bundle failed", result.errors);
    process.exit(1);
  }
}

async function main() {
  // Fail before producing anything: a tree whose tag this script cannot
  // recognise must stop the build, not half-run it.
  const before = readFileSync(indexPath, "utf8");
  if (countScriptTags(before) !== 1) refuse(countScriptTags(before), before.match(SCRIPT_SRC) ?? []);

  await runEsbuild();

  const token = bundleUrlToken(readFileSync(bundlePath));
  const result = versionScriptTag(before, token);
  if (result.matched !== 1) refuse(result.matched, result.tags);
  // Guarded write (rule 2 above): identical content must not bump the mtime of
  // a file the build script watches.
  if (result.changed) writeFileSync(indexPath, result.html);
  console.log(`bundled app.bundle.js?v=${token}`);
}

// Only bundle when this file IS the entry point: check-bundle.mjs imports the
// pure functions above, and a bundler that rebuilt the world because a test read
// one function would be its own defect.
//
// Measured on this host (node v24.19.0): `node bundle.mjs` reports
// import.meta.main === true and importing the module from another script reports
// false, so the flag is authoritative here. The argv comparison is the fallback
// for a node without the flag, and it errs toward RUNNING the bundler: an
// import that also bundled would merely be slow, whereas an entry that silently
// skipped is the exact defect this file was rewritten for.
const isEntry =
  typeof import.meta.main === "boolean"
    ? import.meta.main
    : !process.argv[1] ||
      pathToFileURL(resolve(process.argv[1])).href === import.meta.url;

if (isEntry) {
  await main();
}
