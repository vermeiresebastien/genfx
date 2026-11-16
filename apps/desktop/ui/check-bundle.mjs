// Gate for apps/desktop/ui/bundle.mjs (I5 item 2).
//
// Node-only, no cargo, no GUI, no network: it drives the bundler's own
// decision — the one the old script made silently — over inputs crafted to
// disagree with it. Four of the legs run the REAL bundler as a child process in
// a throwaway copy of the ui directory, because "exits nonzero when nothing was
// substituted" is a property of the process, not of a function.
//
// Usage: node check-bundle.mjs        (CI step + infrastructure/verify.ps1 step)

import {
  copyFileSync,
  mkdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

import { bundleUrlToken, countScriptTags, versionScriptTag } from "./bundle.mjs";

const here = dirname(fileURLToPath(import.meta.url));

let checks = 0;
let failures = 0;

function check(name, fn) {
  checks += 1;
  try {
    fn();
    console.log(`ok   ${name}`);
  } catch (e) {
    failures += 1;
    console.error(`FAIL ${name}: ${e && e.message ? e.message : e}`);
  }
}

function assert(holds, message) {
  if (!holds) throw new Error(message);
}

function assertContains(haystack, needle, label) {
  assert(
    haystack.includes(needle),
    `${label}: expected to find ${JSON.stringify(needle)} in ${JSON.stringify(haystack)}`,
  );
}

function assertThrows(fn, message) {
  let threw = false;
  try {
    fn();
  } catch {
    threw = true;
  }
  assert(threw, message);
}

const TOKEN = "0123456789ab"; // stand-in content hash for the pure legs
const OTHER = "ffffffffffff"; // 12 hex, the width bundleUrlToken() actually emits
for (const fixture of [TOKEN, OTHER]) {
  // Fixture-validity pre-check, not decoration: the first draft of this file
  // used a 14-fixture here, so the hashed-name leg failed on a malformed input
  // and looked like a matcher defect. The hashed-name form is defined as 12 hex
  // because that is what bundleUrlToken() writes.
  if (!/^[0-9a-f]{12}$/.test(fixture)) {
    console.error(`FAIL fixture ${fixture} is not 12 hex — the legs below would be testing shape`);
    process.exit(1);
  }
}

// --- pure legs: the matcher, driven on inputs the old regex could not see ---

check('a plain <script src="app.bundle.js"> (no ?v= at all) gets versioned', () => {
  // The input the old pattern silently ignored: it required `?v=`, so the
  // replace matched nothing, the hash was printed anyway and the script exited
  // 0. A bundle URL with no token is a bundle served from cache.
  const html = `<html><body><script src="app.bundle.js"></script></body></html>`;
  const r = versionScriptTag(html, TOKEN);
  assert(r.matched === 1, `expected 1 matched tag, got ${r.matched}`);
  assert(r.changed, "a tag with no token must change when one is added");
  assertContains(r.html, `src="app.bundle.js?v=${TOKEN}"`, "rewritten html");
  assert(countScriptTags(r.html) === 1, "the rewritten tag must still be recognisable");
});

check("a stale ?v= token is replaced, not left alone", () => {
  const html = `<script src="app.bundle.js?v=${OTHER}" onerror="keep me"></script>`;
  const r = versionScriptTag(html, TOKEN);
  assert(r.matched === 1, `expected 1 matched tag, got ${r.matched}`);
  assertContains(r.html, `src="app.bundle.js?v=${TOKEN}"`, "rewritten html");
  assert(!r.html.includes(OTHER), `the stale token ${OTHER} is still in the output`);
  // Only the src value moves; the sibling attribute is not collateral.
  assertContains(r.html, `onerror="keep me"`, "unrelated attributes survive");
});

check("the hashed-file-name form is accepted and normalised to the real artifact", () => {
  const html = `<script src="app.bundle.${OTHER}.js?v=${OTHER}"></script>`;
  const r = versionScriptTag(html, TOKEN);
  assert(r.matched === 1, `expected 1 matched tag, got ${r.matched}`);
  assertContains(r.html, `src="app.bundle.js?v=${TOKEN}"`, "rewritten html");
  assert(
    !r.html.includes(`app.bundle.${OTHER}.js`),
    "the hashed file name must not survive: nobody writes that file",
  );
});

check("an html with no app.bundle tag reports ZERO, which is the refusal input", () => {
  // Precondition for the end-to-end refusal leg: asserted separately so a
  // matcher that quietly stops matching cannot make that leg vacuous.
  const html = `<html><body><script src="monitor.js"></script></body></html>`;
  assert(countScriptTags(html) === 0, `expected 0 tags, got ${countScriptTags(html)}`);
  const r = versionScriptTag(html, TOKEN);
  assert(r.matched === 0, `expected 0 matched, got ${r.matched}`);
  assert(!r.changed, "no tag means nothing to write");
});

check("two app.bundle tags are counted as two, which is a refusal", () => {
  const html = `<script src="app.bundle.js"></script><script src="app.bundle.js?v=x"></script>`;
  const r = versionScriptTag(html, TOKEN);
  assert(r.matched === 2, `expected 2 matched tags, got ${r.matched}`);
});

check("the token is a function of the bytes", () => {
  const a = bundleUrlToken(Buffer.from("one"));
  const b = bundleUrlToken(Buffer.from("two"));
  assert(/^[0-9a-f]{12}$/.test(a), `token is not 12 hex: ${a}`);
  assert(a !== b, "two different bundles produced the same token");
  assert(a === bundleUrlToken(Buffer.from("one")), "the token is not reproducible");
});

// --- end-to-end legs: the real script, as a child process, in a throwaway tree ---

const probeRoot = join(here, `.i5-probe-${process.pid}`);
const probeBundle = join(probeRoot, "app.bundle.js");
const probeIndex = join(probeRoot, "index.html");
let afterPlainRun = null; // state the later legs depend on, so they cannot pass vacuously

function makeProbeTree(indexHtml) {
  rmTree();
  // bundle.mjs resolves everything from its own directory, so a copy bundles
  // into the probe tree. esbuild is found by walking up to
  // apps/desktop/ui/node_modules — which is why the probe sits inside ui/ and
  // why it is removed again in the finally block below.
  mkdirSync(probeRoot, { recursive: true });
  copyFileSync(join(here, "bundle.mjs"), join(probeRoot, "bundle.mjs"));
  writeFileSync(join(probeRoot, "main.js"), "export const probe = 1;\n");
  writeFileSync(join(probeRoot, "index.html"), indexHtml);
}

function runProbe() {
  const r = spawnSync(process.execPath, ["bundle.mjs"], {
    cwd: probeRoot,
    encoding: "utf8",
  });
  return { status: r.status, output: `${r.stdout || ""}${r.stderr || ""}` };
}

function probeIndexState() {
  const html = readFileSync(probeIndex, "utf8");
  return {
    html,
    bytes: html.length,
    mtimeMs: statSync(probeIndex).mtimeMs,
    token: bundleUrlToken(readFileSync(probeBundle)),
  };
}

function rmTree() {
  rmSync(probeRoot, { recursive: true, force: true });
}

try {
  check("END-TO-END: no app.bundle tag at all -> the bundler exits NONZERO", () => {
    makeProbeTree(`<html><body><script src="monitor.js"></script></body></html>`);
    const r = runProbe();
    assert(
      r.status !== null && r.status !== 0,
      `expected a nonzero exit when nothing can be substituted, got status=${r.status} ` +
        `(output: ${r.output})`,
    );
    assertContains(r.output, "found 0", "refusal message");
    assertContains(r.output, "exactly one app.bundle.js script tag", "refusal message");
    // It refused before doing work: no bundle was produced.
    assertThrows(() => readFileSync(probeBundle), "the run wrote app.bundle.js after refusing");
  });

  check('END-TO-END: a plain tag is versioned, exit 0, token == sha256(bundle)', () => {
    makeProbeTree(`<html><body><script src="app.bundle.js"></script></body></html>`);
    const r = runProbe();
    assert(r.status === 0, `expected exit 0, got ${r.status} (output: ${r.output})`);
    const token = bundleUrlToken(readFileSync(probeBundle));
    assertContains(readFileSync(probeIndex, "utf8"), `src="app.bundle.js?v=${token}"`, "tag");
    assertContains(r.output, `bundled app.bundle.js?v=${token}`, "marker line agrees");
    assert(countScriptTags(readFileSync(probeIndex, "utf8")) === 1, "one tag remains");
    afterPlainRun = probeIndexState();
  });

  check("END-TO-END: re-running with nothing changed writes NOTHING (no mtime bump)", () => {
    assert(afterPlainRun !== null, "the plain-tag leg did not record state; this leg is vacuous");
    const r = runProbe();
    assert(r.status === 0, `expected exit 0, got ${r.status} (output: ${r.output})`);
    const now = probeIndexState();
    assert(
      now.mtimeMs === afterPlainRun.mtimeMs && now.bytes === afterPlainRun.bytes,
      `index.html was rewritten by a no-op run (mtime ${afterPlainRun.mtimeMs} -> ` +
        `${now.mtimeMs}). That write is what makes every cargo build re-run this script, ` +
        "because apps/desktop/src-tauri/build.rs watches index.html.",
    );
  });

  check("END-TO-END: a changed bundle provably changes the token", () => {
    assert(afterPlainRun !== null, "the plain-tag leg did not record state; this leg is vacuous");
    writeFileSync(join(probeRoot, "main.js"), "export const probe = 2;\n// extra bytes\n");
    const r = runProbe();
    assert(r.status === 0, `expected exit 0, got ${r.status} (output: ${r.output})`);
    const now = probeIndexState();
    assertContains(now.html, `src="app.bundle.js?v=${now.token}"`, "tag moved with the bundle");
    assert(
      now.token !== afterPlainRun.token,
      `the bundle changed but the token did not (${now.token}): the tag is not content-addressed`,
    );
  });

  check("REAL TREE: index.html has one tag and already matches the committed bundle", () => {
    // The `git status` question, stated as an assertion instead of as a report:
    // because the committed ?v= equals sha256(committed app.bundle.js)[0..12]
    // and the write is guarded, a normal build leaves the tracked index.html
    // byte-identical. If this reddens, a build WILL dirty index.html, and the
    // fix is to rebuild and commit both files together.
    const html = readFileSync(join(here, "index.html"), "utf8");
    const n = countScriptTags(html);
    assert(
      n === 1,
      `the tracked index.html has ${n} app.bundle tags; a real build exits 1 on this tree`,
    );
    const token = bundleUrlToken(readFileSync(join(here, "app.bundle.js")));
    assertContains(html, `src="app.bundle.js?v=${token}"`, "committed index.html vs bundle");
  });
} finally {
  rmTree();
}

if (failures) {
  console.error(`bundle cache-buster checks: ${checks - failures}/${checks} passed — FAIL`);
  process.exit(1);
}
console.log(`bundle cache-buster checks: ${checks}/${checks} passed`);
