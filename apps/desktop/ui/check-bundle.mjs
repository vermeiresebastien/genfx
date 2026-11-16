// Gate for apps/desktop/ui/bundle.mjs (I5 item 2, extended by I15 items 1-2).
//
// Node-only, no cargo, no GUI, no network: it drives the bundler's own
// decision — the one the old script made silently — over inputs crafted to
// disagree with it. Thirteen of the legs run the REAL bundler as a child
// process in a throwaway copy of the ui directory, because "exits nonzero when
// nothing was substituted" is a property of the process, not of a function.
// Three more call esbuild in-process, through bundle.mjs's own exported
// BUNDLE_OPTIONS: the freshness comparison (item 2) has to build with the same
// options the build uses, or it is comparing a program nobody ships.
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

import {
  bundleUrlToken,
  countScriptTags,
  versionScriptTag,
  bundleFromSources,
  staleReport,
} from "./bundle.mjs";

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

// The freshness legs have to AWAIT esbuild, so they need the same bookkeeping
// one level up. Both counters are shared: one number, one verdict, no leg that
// can report a result the summary does not count.
async function checkAsync(name, fn) {
  checks += 1;
  try {
    await fn();
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

// --- pure legs: the freshness comparison (I15 item 2) ---

check("FRESHNESS: byte-identical is fresh, and that is the whole claim", () => {
  const src = Buffer.from("var x = 1;\nconsole.log(x);\n");
  const r = staleReport(src, Buffer.from(src));
  assert(r.fresh === true, `identical bytes reported stale: ${JSON.stringify(r)}`);
  assert(r.lineEndingsOnly === false, "identical bytes cannot be a line-endings case");
  assert(r.detail === "", `a fresh comparison should have nothing to say, said ${r.detail}`);
});

check("FRESHNESS: same length, different program is STALE — the case an mtime rule cannot see", () => {
  // The fixture is deliberately the same size on both sides. An
  // mtime(bundle) >= mtime(inputs) rule reads this tree as fresh whenever the
  // bundle was touched after the edit, which is every tree a skipped rebuild
  // produces; a byte digest has no such blind spot.
  const src = Buffer.from("var x = 1;\nvar y = 2;\n");
  const art = Buffer.from("var x = 1;\nvar y = 3;\n");
  assert(src.length === art.length, "fixture is no longer same-length; the point is lost");
  const r = staleReport(src, art);
  assert(r.fresh === false, `a one-byte program difference read as fresh: ${JSON.stringify(r)}`);
  assertContains(r.detail, "first difference at byte 19", "staleness detail");
  assertContains(r.detail, `${src.length} bytes`, "both sizes are named");
});

check("FRESHNESS: line endings alone are reported, never called stale", () => {
  // The hosted checkout can rewrite a tracked file's separators without
  // changing its program (measured on this host with app.bundle.js). This
  // comparison is about the program; the tag-vs-served-bytes leg owns the
  // other claim, and saying so here keeps the two from being confused.
  const r = staleReport(Buffer.from("a\nb\n"), Buffer.from("a\r\nb\r\n"));
  assert(r.fresh === true, `a line-endings-only difference was called stale: ${JSON.stringify(r)}`);
  assert(r.lineEndingsOnly === true, "the line-endings case must be visible in the result");
  assertContains(r.detail, "different line endings", "report detail");
});

// --- end-to-end legs: the real script, as a child process, in a throwaway tree ---

const probeRoot = join(here, `.i5-probe-${process.pid}`);
const probeBundle = join(probeRoot, "app.bundle.js");
const probeIndex = join(probeRoot, "index.html");
const probeMonitor = join(probeRoot, "monitor.html");
const probeMonitorScript = join(probeRoot, MONITOR);
let afterPlainRun = null; // state the later legs depend on, so they cannot pass vacuously
let afterPlainMonitorRun = null;
let stalenessSeen = null; // the report the stale-artifact leg produced, for its sequel

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

  // --- freshness legs: bundle vs the sources that produced it (I15 item 2) ---

  await checkAsync(
    "FRESHNESS END-TO-END: an input moves, nobody re-bundles, and the gate says STALE",
    async () => {
      makeProbeTree();
      const first = runProbe();
      assert(first.status === 0, `setup run exited ${first.status} (output: ${first.output})`);
      const built = await bundleFromSources(probeRoot);
      assert(
        staleReport(built, readFileSync(probeBundle)).fresh,
        "a tree that was just bundled does not read fresh — the comparison itself is broken",
      );
      // The defect, in its own words: an input changes and the artifact does
      // not. Nothing else in this file looks at this pair.
      writeFileSync(join(probeRoot, "main.js"), "export const probe = 3;\n// moved\n");
      const moved = await bundleFromSources(probeRoot);
      const report = staleReport(moved, readFileSync(probeBundle));
      assert(
        report.fresh === false,
        `editing main.js left the artifact reading FRESH (${JSON.stringify(report)}): the ` +
          "gate cannot see the inputs it is supposed to compare",
      );
      assertContains(report.detail, "first difference at byte", "staleness detail");
      stalenessSeen = report;
    },
  );

  await checkAsync("FRESHNESS END-TO-END: re-running the bundler makes the same tree fresh", () => {
    assert(stalenessSeen !== null, "the stale-artifact leg recorded nothing; this leg is vacuous");
    const r = runProbe();
    assert(r.status === 0, `expected exit 0, got ${r.status} (output: ${r.output})`);
    return bundleFromSources(probeRoot).then((bytes) => {
      const report = staleReport(bytes, readFileSync(probeBundle));
      assert(report.fresh, `after a real run the artifact still reads stale: ${report.detail}`);
      assertContains(bytes.toString("utf8"), "probe = 3", "the rebuilt bundle carries the edit");
    });
  });

  await checkAsync("FRESHNESS END-TO-END: a no-op run does not rewrite app.bundle.js either", () => {
    // The write guard exists for index.html and monitor.html; app.bundle.js is
    // tracked too, so rewriting identical bytes there is the same class of
    // noise — a build that cannot be told apart from one that shipped a
    // different program.
    const before = { mtimeMs: statSync(probeBundle).mtimeMs, bytes: readFileSync(probeBundle) };
    const r = runProbe();
    assert(r.status === 0, `expected exit 0, got ${r.status} (output: ${r.output})`);
    const after = readFileSync(probeBundle);
    assert(
      statSync(probeBundle).mtimeMs === before.mtimeMs && after.equals(before.bytes),
      `app.bundle.js was rewritten by a no-op run (mtime ${before.mtimeMs} -> ` +
        `${statSync(probeBundle).mtimeMs}); the bundle write is not guarded`,
    );
  });

  await checkAsync(
    "FRESHNESS REAL TREE: the committed app.bundle.js is what the committed sources produce",
    async () => {
      // What this arm cannot see, stated where the reader decides to trust it:
      //   * only main.js's import graph. style.css, monitor.js, monitor.html,
      //     package.json and tauri.conf.json are not inputs to this digest —
      //     and none of them can make app.bundle.js stale. monitor.js got its
      //     own token in item 1; the rest are covered only by build.rs's
      //     rerun-if-changed list.
      //   * a touched-but-identical file reads fresh, which is the correct
      //     answer and the reason this is a digest and not an mtime rule.
      //   * a different esbuild (node_modules is watched by nothing) produces
      //     different bytes from the same sources and reddens this leg with a
      //     size/offset report; it cannot tell you WHICH input moved.
      //   * it reads the worktree, so it says nothing about what an installer
      //     someone built last week shipped.
      // The option set is shared with the build by construction — BUNDLE_OPTIONS
      // is the only copy — which is the point: if the build ever stops using it,
      // this comparison is the leg that notices.
      const bytes = await bundleFromSources(here);
      assert(
        bytes.length > 1000,
        `an in-memory bundle of ${bytes.length} bytes is not the app graph; this leg would ` +
          "then be comparing the wrong artifact",
      );
      const report = staleReport(bytes, readFileSync(join(here, "app.bundle.js")));
      if (report.lineEndingsOnly) console.log(`     note: ${report.detail}`);
      assert(
        report.fresh,
        `STALE BUNDLE — ${report.detail}. A desktop build embeds apps/desktop/ui/app.bundle.js, ` +
          "so this tree ships a UI that is not its sources. Run the bundler and commit both " +
          "files together.",
      );
    },
  );

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
