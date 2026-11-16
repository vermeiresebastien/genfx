// Rebuilds apps/desktop/ui/app.bundle.js from the ES-module entry (main.js)
// and rewrites the version query on EVERY served script URL with a content
// hash, so a new artifact is never served from a stale WebView2 cache.
//
// Usage: node bundle.mjs
// Imported by check-bundle.mjs for the pure parts (see isEntry at the bottom).
//
// Four rules this file exists to obey, and the one it broke at 63bc321:
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
//      fresh baseline to compare against at all. monitor.html sits on that same
//      watch list, so the guard covers it too — versioning the monitor is only
//      safe BECAUSE the write is guarded.
//   3. The rule is about SERVED ARTIFACTS, not about the bundler. Each window
//      loads exactly one script, and the token must come from the bytes the
//      webview actually requests. That is why this is a per-file table rather
//      than a second esbuild entry: monitor.js has no imports at all (2,122
//      bytes of top-level statements), so bundling it would wrap those same
//      bytes in an IIFE, change the artifact on disk, and move the Tauri config
//      and the `WebviewUrl::App("monitor.html")` that names it. Versioning the
//      file that is already served has no such blast radius.
//   4. The artifact's bytes must not depend on WHERE the bundler was started.
//      esbuild prints one `// <path>` comment per module, relative to its
//      working directory, so the committed sources measured 98,555 bytes from
//      apps/desktop/ui (which is what build.rs runs it with) and 98,747 from the
//      repo root — the same program, two artifacts, both tracked. absWorkingDir
//      is therefore pinned in BUNDLE_OPTIONS, and BUNDLE_OPTIONS is exported so
//      the freshness gate re-bundles through THE SAME options instead of a
//      copy that can drift. That pinning is what makes a byte digest usable as
//      a freshness claim at all.
//
// What is deliberately NOT done here: the hashed-file NAME form
// (`app.bundle.<hash>.js`) is accepted on the left and normalised away on the
// right, for every row of the table. The artifacts on disk are
// apps/desktop/ui/app.bundle.js and apps/desktop/ui/monitor.js, and Tauri
// embeds those paths, so emitting a hashed filename would point a tag at a file
// nobody writes. Only the query carries the version.

import { createHash } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));

// What gets versioned: one row per HTML document and the single script it
// serves. `built: true` marks the artifact this run produces, so the pre-flight
// does not refuse over a bundle that is legitimately not on disk yet; in every
// other row the script's own bytes ARE the token, so it must already exist.
// `marker` prefixes the line this run prints per row — the proof the row was
// processed, and what check-bundle.mjs asserts.
export const VERSIONED = [
  { html: "index.html", script: "app.bundle.js", built: true, marker: "bundled" },
  { html: "monitor.html", script: "monitor.js", built: false, marker: "versioned" },
];

// The bundle's path and the options that produce it. Exported because the
// freshness gate (check-bundle.mjs, I15 item 2) must build the SAME bytes the
// build writes: two option objects means two programs, and a gate that builds
// something else than the build ships proves nothing.
export const bundlePath = join(here, "app.bundle.js");
export const BUNDLE_OPTIONS = {
  bundle: true,
  format: "iife",
  target: "es2020",
  logLevel: "silent",
  platform: "browser",
  // Pinned, not inherited: see rule 4 above. Callers that work on a different
  // directory (a probe tree) override this with that directory.
  absWorkingDir: here,
};

// The script src attribute for one row, whether or not it carries a version
// query and whether or not it carries the hashed-file form of the name.
// Global on purpose: countScriptTags IS the assertion, and a non-global regex
// would report 1 for any input containing at least one tag, which makes
// "exactly one" true by construction — the same vacuity as asserting a
// substring that both the success and the refusal path print.
function tagPattern(script) {
  const dot = script.lastIndexOf(".");
  const escape = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const base = escape(script.slice(0, dot));
  const ext = escape(script.slice(dot));
  return new RegExp(`src="${base}(?:\\.[0-9a-f]{12})?${ext}(?:\\?v=[^"]*)?"`, "g");
}

/** The version token: 12 hex of the sha256 of the bytes actually served.
 *  Content-addressed, so a changed artifact provably changes the tag and an
 *  unchanged one provably does not. */
export function bundleUrlToken(bytes) {
  return createHash("sha256").update(bytes).digest("hex").slice(0, 12);
}

/** How many `script` src attributes this HTML carries. The default is the
 *  bundle row, so callers written before the table keep their exact meaning. */
export function countScriptTags(html, script = "app.bundle.js") {
  return (html.match(tagPattern(script)) ?? []).length;
}

/**
 * Point every `script` tag in this HTML at `token`.
 *
 * Returns the rewritten HTML plus how many tags it matched, so the caller can
 * insist on exactly one instead of assuming it. `changed` is what decides
 * whether the HTML gets written.
 */
export function versionScriptTag(html, token, script = "app.bundle.js") {
  const pattern = tagPattern(script);
  const tags = html.match(pattern) ?? [];
  const out = html.replace(pattern, `src="${script}?v=${token}"`);
  return { html: out, matched: tags.length, tags, changed: out !== html };
}

function refusalMessage(target, count, tags) {
  return (
    `${target.html} must carry exactly one ${target.script} script tag; found ${count} ` +
    `[${tags.join(", ") || "no match"}]. ` +
    (count === 0
      ? "Nothing was versioned: the next changed artifact would be served from " +
        "WebView2's cache, which is the failure this script exists to prevent."
      : "One token cannot version two tags, and guessing which one you meant " +
        "is not this script's call.")
  );
}

function refuse(refusals) {
  // Every refusal, then one exit. Reporting only the first would make a tree
  // with two broken documents take two runs to describe — and a run that is
  // green on the second try says nothing about why the first was not.
  for (const line of refusals) console.error(line);
  process.exit(1);
}

/**
 * The bytes `dir`'s ES-module entry produces, built in memory and NEVER
 * written. Shared by the bundler and by the freshness gate.
 *
 * esbuild is imported dynamically so that reading the pure functions above
 * does not require it to be installed. When it is missing this THROWS rather
 * than exiting: the bundler turns that into a nonzero exit (its own failure
 * path below), and a gate that cannot build must report a failed leg, not a
 * skipped one — a skipped freshness check is the same D036 defect as one that
 * prints a hash and means nothing.
 */
export async function bundleFromSources(dir = here) {
  let build;
  try {
    ({ build } = await import("esbuild"));
  } catch (e) {
    throw new Error(
      `esbuild could not be loaded from apps/desktop/ui (${e.message}). Run ` +
        `"npm ci" in apps/desktop/ui; neither the build nor the freshness gate ` +
        `falls back to the committed bundle.`,
    );
  }
  const result = await build({
    ...BUNDLE_OPTIONS,
    absWorkingDir: dir,
    entryPoints: [join(dir, "main.js")],
    write: false,
  });
  if (result.errors && result.errors.length) {
    throw new Error(`esbuild reported ${result.errors.length} error(s) building ${dir}/main.js`);
  }
  return Buffer.from(result.outputFiles[0].text, "utf8");
}

const toLF = (bytes) => Buffer.from(bytes.toString("utf8").split("\r\n").join("\n"), "utf8");

/**
 * Is `artifact` the program `sources` produces?
 *
 * `fresh` compares CONTENT, because that is the question item 2 asks: an
 * artifact whose bytes equal what the inputs produce is not stale, whatever
 * its mtime says. `lineEndingsOnly` is reported separately and does NOT make a
 * run stale: the hosted checkout can rewrite a tracked file's separators
 * (measured on this host: `git checkout -- app.bundle.js` under
 * core.autocrlf=true produced a copy whose sha256 moved
 * bd750f689077 -> 7c7d333c210f with no code change), and the tag-vs-bytes leg
 * in check-bundle.mjs is the arm that owns the served-bytes claim.
 */
export function staleReport(sources, artifact) {
  if (sources.equals(artifact)) return { fresh: true, lineEndingsOnly: false, detail: "" };
  if (toLF(sources).equals(toLF(artifact))) {
    return {
      fresh: true,
      lineEndingsOnly: true,
      detail:
        `identical program, different line endings (sources ${sources.length} bytes, ` +
        `artifact ${artifact.length} bytes)`,
    };
  }
  let offset = 0;
  while (offset < Math.min(sources.length, artifact.length) && sources[offset] === artifact[offset]) {
    offset += 1;
  }
  return {
    fresh: false,
    lineEndingsOnly: false,
    detail:
      `the sources produce ${sources.length} bytes, the artifact on disk is ` +
      `${artifact.length} bytes; first difference at byte ${offset}`,
  };
}

async function runEsbuild() {
  // The bundle write is guarded for the same reason the HTML writes are (rule
  // 2): app.bundle.js is a tracked file, so an unconditional rewrite made every
  // no-op build bump its mtime — and with a content-addressed tag, a build that
  // rewrote identical bytes was indistinguishable from one that shipped a
  // different program.
  let bytes;
  try {
    bytes = await bundleFromSources(here);
  } catch (e) {
    // apps/desktop/src-tauri/build.rs (I5 item 1) turns this nonzero exit into a
    // build failure instead of embedding the old bundle.
    console.error(String(e.message || e));
    process.exit(1);
  }
  const onDisk = existsSync(bundlePath) ? readFileSync(bundlePath) : null;
  if (!onDisk || !onDisk.equals(bytes)) writeFileSync(bundlePath, bytes);
}

async function main() {
  // Fail before producing anything: a tree whose tags this script cannot
  // recognise must stop the build, not half-run it. Every row is checked, so a
  // single run describes every broken document in the tree.
  const refusals = [];
  const pages = new Map();
  for (const target of VERSIONED) {
    const htmlPath = join(here, target.html);
    let text;
    try {
      text = readFileSync(htmlPath, "utf8");
    } catch (e) {
      refusals.push(
        `${target.html} could not be read (${e.code || e.message}). ${target.script} ` +
          "cannot be versioned without the document that loads it, and this script " +
          "does not write an HTML file of its own.",
      );
      continue;
    }
    if (!target.built && !existsSync(join(here, target.script))) {
      refusals.push(
        `${target.script} is not on disk in ${here}, so ${target.html} has no bytes to ` +
          "hash into a token. Fix the path in VERSIONED or restore the file: shipping an " +
          "untagged tag is the defect this script exists to prevent.",
      );
      continue;
    }
    const tags = text.match(tagPattern(target.script)) ?? [];
    if (tags.length !== 1) refusals.push(refusalMessage(target, tags.length, tags));
    else pages.set(target, text);
  }
  if (refusals.length) refuse(refusals);

  await runEsbuild();

  for (const target of VERSIONED) {
    // The token is read off the bytes on disk AFTER the build that writes them,
    // never recomputed from the source graph: the tag must describe what is
    // actually served. For the built row, a failed runEsbuild() has already
    // exited nonzero rather than reached here without an artifact.
    const token = bundleUrlToken(readFileSync(join(here, target.script)));
    const result = versionScriptTag(pages.get(target), token, target.script);
    if (result.matched !== 1) refuse([refusalMessage(target, result.matched, result.tags)]);
    // Guarded write (rule 2 above): identical content must not bump the mtime of
    // a file the build script watches.
    if (result.changed) writeFileSync(join(here, target.html), result.html);
    console.log(`${target.marker} ${target.script}?v=${token}`);
  }
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
