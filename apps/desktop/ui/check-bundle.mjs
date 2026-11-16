// Gate for apps/desktop/ui/bundle.mjs (I5 item 2, extended by I15 item 1).
//
// Node-only, no cargo, no GUI, no network: it drives the bundler's own
// decision — the one the old script made silently — over inputs crafted to
// disagree with it. Ten of the legs run the REAL bundler as a child process in
// a throwaway copy of the ui directory, because "exits nonzero when nothing was
// substituted" is a property of the process, not of a function.
//
// I15 item 1 is the second row of bundle.mjs's VERSIONED table: monitor.html
// used to load a plain, unversioned monitor.js, so a changed monitor script
// could be masked by WebView2's cache while the main window was correctly
// versioned. Every monitor leg below therefore names the document it refused
// in its assertions, and each also asserts the OTHER document was NOT blamed —
// a refusal that does not say which row fired would let a monitor rule be
// satisfied by an index.html failure.
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
const MONITOR = "monitor.js"; // the row I15 adds; the default arg is the bundle row
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

// --- pure legs, monitor row (I15 item 1) ---

check('MONITOR: a plain <script src="monitor.js"> gets versioned, and is NOT an app.bundle tag', () => {
  // The exact shape monitor.html carried before this item: a plain tag, so a
  // changed monitor.js was served from cache with a green bundler. The second
  // half is the cross-match guard — if one pattern could see the other file,
  // "exactly one tag" would be satisfiable by the wrong script.
  const html = `<html><body><script src="${MONITOR}"></script></body></html>`;
  const r = versionScriptTag(html, TOKEN, MONITOR);
  assert(r.matched === 1, `expected 1 matched monitor tag, got ${r.matched}`);
  assert(r.changed, "a monitor tag with no token must change when one is added");
  assertContains(r.html, `src="${MONITOR}?v=${TOKEN}"`, "rewritten monitor html");
  assert(countScriptTags(r.html, MONITOR) === 1, "the rewritten tag is still recognisable");
  assert(countScriptTags(html) === 0, "the app.bundle pattern must not see a monitor tag");
  assert(
    countScriptTags(versionScriptTag(`<script src="app.bundle.js"></script>`, TOKEN).html, MONITOR) === 0,
    "the monitor pattern must not see an app.bundle tag",
  );
});

check("MONITOR: a stale ?v= token is replaced, and unrelated attributes survive", () => {
  const html = `<script src="${MONITOR}?v=${OTHER}" onerror="keep me"></script>`;
  const r = versionScriptTag(html, TOKEN, MONITOR);
  assert(r.matched === 1, `expected 1 matched monitor tag, got ${r.matched}`);
  assertContains(r.html, `src="${MONITOR}?v=${TOKEN}"`, "rewritten monitor html");
  assert(!r.html.includes(OTHER), `the stale monitor token ${OTHER} is still in the output`);
  assertContains(r.html, `onerror="keep me"`, "unrelated attributes survive");
});

check("MONITOR: the hashed-file-name form is accepted and normalised too", () => {
  // Same left-side tolerance as the bundle row, gated on its own script name:
  // the pattern is built from the row, so this is the second caller of it, not
  // a re-run of the first.
  const html = `<script src="monitor.${OTHER}.js?v=${OTHER}"></script>`;
  const r = versionScriptTag(html, TOKEN, MONITOR);
  assert(r.matched === 1, `expected 1 matched monitor tag, got ${r.matched}`);
  assertContains(r.html, `src="${MONITOR}?v=${TOKEN}"`, "rewritten monitor html");
  assert(!r.html.includes(`monitor.${OTHER}.js`), "the hashed file name must not survive");
});

check("MONITOR: zero and two tags are counted as the refusal inputs they are", () => {
  // Preconditions for the two end-to-end monitor refusal legs, asserted
  // separately so neither can pass by construction.
  const none = `<html><body><p>no script at all</p></body></html>`;
  assert(countScriptTags(none, MONITOR) === 0, `expected 0 tags, got ${countScriptTags(none, MONITOR)}`);
  const two = `<script src="${MONITOR}"></script><script src="${MONITOR}?v=x"></script>`;
  assert(countScriptTags(two, MONITOR) === 2, `expected 2 tags, got ${countScriptTags(two, MONITOR)}`);
  const r = versionScriptTag(two, TOKEN, MONITOR);
  assert(r.matched === 2, `expected 2 matched, got ${r.matched}`);
});

// --- end-to-end legs: the real script, as a child process, in a throwaway tree ---

const probeRoot = join(here, `.i5-probe-${process.pid}`);
const probeBundle = join(probeRoot, "app.bundle.js");
const probeIndex = join(probeRoot, "index.html");
const probeMonitor = join(probeRoot, "monitor.html");
const probeMonitorScript = join(probeRoot, MONITOR);
let afterPlainRun = null; // state the later legs depend on, so they cannot pass vacuously
let afterPlainMonitorRun = null;

const PLAIN_INDEX = `<html><body><script src="app.bundle.js"></script></body></html>`;
const PLAIN_MONITOR = `<html><body><script src="${MONITOR}"></script></body></html>`;

function makeProbeTree(indexHtml = PLAIN_INDEX, monitorHtml = PLAIN_MONITOR) {
  rmTree();
  // bundle.mjs resolves everything from its own directory, so a copy bundles
  // into the probe tree. esbuild is found by walking up to
  // apps/desktop/ui/node_modules — which is why the probe sits inside ui/ and
  // why it is removed again in the finally block below.
  mkdirSync(probeRoot, { recursive: true });
  copyFileSync(join(here, "bundle.mjs"), join(probeRoot, "bundle.mjs"));
  writeFileSync(join(probeRoot, "main.js"), "export const probe = 1;\n");
  writeFileSync(join(probeRoot, "index.html"), indexHtml);
  // The table has a second row now. A tree that carried no monitor pair would
  // refuse for a reason the leg it is feeding is not testing, so every probe
  // gets a valid monitor document and script unless the leg overrides them.
  writeFileSync(probeMonitorScript, "document.title = 'monitor probe';\n");
  writeFileSync(probeMonitor, monitorHtml);
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

function probeMonitorState() {
  const html = readFileSync(probeMonitor, "utf8");
  return {
    html,
    bytes: html.length,
    mtimeMs: statSync(probeMonitor).mtimeMs,
    // Read off monitor.js's bytes, which is the file the tag must describe.
    token: bundleUrlToken(readFileSync(probeMonitorScript)),
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
    // Attribution: the monitor row of this tree is valid, so it must NOT be
    // blamed. A refusal that names the wrong document would let this leg pass
    // while the app rule was broken.
    assert(!r.output.includes("monitor.html"), `the refusal blames monitor.html: ${r.output}`);
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

  // --- end-to-end legs, monitor row (I15 item 1) ---

  check("MONITOR END-TO-END: a plain tag is versioned from monitor.js's bytes, exit 0", () => {
    makeProbeTree(PLAIN_INDEX, PLAIN_MONITOR);
    const r = runProbe();
    assert(r.status === 0, `expected exit 0, got ${r.status} (output: ${r.output})`);
    // The token is recomputed here from the file the tag points at. If the
    // bundler hashed something else — index.html, the bundle, nothing — this
    // is red, so the leg is not true by construction.
    const token = bundleUrlToken(readFileSync(probeMonitorScript));
    assertContains(readFileSync(probeMonitor, "utf8"), `src="${MONITOR}?v=${token}"`, "monitor tag");
    assertContains(r.output, `versioned ${MONITOR}?v=${token}`, "marker line agrees");
    assert(
      token !== bundleUrlToken(readFileSync(probeBundle)),
      "the monitor row was versioned with the bundle's token, not monitor.js's own bytes",
    );
    assert(countScriptTags(readFileSync(probeMonitor, "utf8"), MONITOR) === 1, "one tag remains");
    afterPlainMonitorRun = probeMonitorState();
  });

  check("MONITOR END-TO-END: re-running with nothing changed does not touch monitor.html", () => {
    assert(
      afterPlainMonitorRun !== null,
      "the plain monitor-tag leg did not record state; this leg is vacuous",
    );
    const r = runProbe();
    assert(r.status === 0, `expected exit 0, got ${r.status} (output: ${r.output})`);
    const now = probeMonitorState();
    assert(
      now.mtimeMs === afterPlainMonitorRun.mtimeMs && now.bytes === afterPlainMonitorRun.bytes,
      `monitor.html was rewritten by a no-op run (mtime ${afterPlainMonitorRun.mtimeMs} -> ` +
        `${now.mtimeMs}). monitor.html is a cargo:rerun-if-changed input of ` +
        "apps/desktop/src-tauri/build.rs exactly like index.html, so that write would make " +
        "every desktop build re-run the bundler.",
    );
  });

  check("MONITOR END-TO-END: a changed monitor.js provably changes its token", () => {
    assert(
      afterPlainMonitorRun !== null,
      "the plain monitor-tag leg did not record state; this leg is vacuous",
    );
    writeFileSync(probeMonitorScript, "document.title = 'monitor probe 2'; // extra bytes\n");
    const r = runProbe();
    assert(r.status === 0, `expected exit 0, got ${r.status} (output: ${r.output})`);
    const now = probeMonitorState();
    assertContains(now.html, `src="${MONITOR}?v=${now.token}"`, "tag moved with the script");
    assert(
      now.token !== afterPlainMonitorRun.token,
      `monitor.js changed but its token did not (${now.token}): the monitor tag is not ` +
        "content-addressed, which is the defect this item exists to close",
    );
  });

  check("MONITOR END-TO-END: no monitor tag -> NONZERO, and index.html is not blamed", () => {
    makeProbeTree(PLAIN_INDEX, `<html><body><p>the monitor window lost its script</p></body></html>`);
    const r = runProbe();
    assert(
      r.status !== null && r.status !== 0,
      `expected a nonzero exit when the monitor tag is gone, got status=${r.status} ` +
        `(output: ${r.output})`,
    );
    assertContains(r.output, "monitor.html must carry exactly one monitor.js script tag", "refusal");
    assertContains(r.output, "found 0", "refusal count");
    assert(!r.output.includes("index.html must"), `the refusal blames index.html: ${r.output}`);
    assertThrows(() => readFileSync(probeBundle), "the run wrote app.bundle.js after refusing");
  });

  check("MONITOR END-TO-END: two monitor tags -> NONZERO, and index.html is not blamed", () => {
    makeProbeTree(PLAIN_INDEX, `<script src="${MONITOR}"></script><script src="${MONITOR}?v=x"></script>`);
    const r = runProbe();
    assert(
      r.status !== null && r.status !== 0,
      `expected a nonzero exit when one token would version two tags, got status=${r.status} ` +
        `(output: ${r.output})`,
    );
    assertContains(r.output, "found 2", "refusal count");
    assertContains(r.output, "One token cannot version two tags", "refusal reason");
    assert(!r.output.includes("index.html must"), `the refusal blames index.html: ${r.output}`);
    assertThrows(() => readFileSync(probeBundle), "the run wrote app.bundle.js after refusing");
  });

  check("MONITOR END-TO-END: monitor.js missing -> NONZERO, nothing versioned", () => {
    // The row's second precondition: a token needs bytes. Without this arm the
    // existsSync branch in bundle.mjs would be new, unexercised code.
    makeProbeTree();
    rmSync(probeMonitorScript);
    const r = runProbe();
    assert(
      r.status !== null && r.status !== 0,
      `expected a nonzero exit when there are no bytes to hash, got status=${r.status} ` +
        `(output: ${r.output})`,
    );
    assertContains(r.output, "monitor.js is not on disk", "refusal reason");
    assertThrows(() => readFileSync(probeBundle), "the run wrote app.bundle.js after refusing");
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

  check("REAL TREE: monitor.html has one tag and it equals the committed monitor.js bytes", () => {
    // The I15 item 1 acceptance leg. Two things make it more than a tautology:
    // the token is hashed from monitor.js while it is asserted against
    // monitor.html, and the last assertion requires the two hashes to differ —
    // an implementation that hashed the document it writes into would satisfy
    // the substring check and still be red here.
    const html = readFileSync(join(here, "monitor.html"), "utf8");
    const n = countScriptTags(html, MONITOR);
    assert(
      n === 1,
      `the tracked monitor.html has ${n} monitor.js tags; a real build exits 1 on this tree`,
    );
    const token = bundleUrlToken(readFileSync(join(here, MONITOR)));
    assertContains(html, `src="${MONITOR}?v=${token}"`, "committed monitor.html vs monitor.js");
    // The cross-row discriminator. A per-file table's specific failure mode is
    // reusing the row above it, and nothing else here would notice: the tag
    // would be 12 hex, present, and stable across runs. An implementation that
    // hashed the *document* instead of the script is already ruled out by the
    // assertion above — no document hash equals a different file's hash unless
    // the two files are byte-identical, so that arm needed no extra assert.
    assert(
      token !== bundleUrlToken(readFileSync(join(here, "app.bundle.js"))),
      "the monitor tag carries the BUNDLE's hash: the table reuses the row above it",
    );
    assert(
      countScriptTags(html) === 0,
      "the tracked monitor.html carries an app.bundle tag, which the bundler would refuse on",
    );
  });
} finally {
  rmTree();
}

if (failures) {
  console.error(`bundle cache-buster checks: ${checks - failures}/${checks} passed — FAIL`);
  process.exit(1);
}
console.log(`bundle cache-buster checks: ${checks}/${checks} passed`);
