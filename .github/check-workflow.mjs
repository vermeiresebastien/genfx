// Structural checks over a GitHub Actions workflow, plus the mutation controls that
// prove each check still fires.
//
// Node-only and dependency-free on purpose: neither Cargo.lock nor apps/desktop holds a
// YAML parser, so this is a scan of the subset this workflow actually uses (block
// mappings, block sequences, block scalars, `#` comments), not a YAML parser. It exists
// because a workflow GitHub rejects produces zero jobs and zero logs, so every one of
// these faults was paid for with a hosted round trip: a duplicated `shell:` key that
// made the file unreadable, and a job id spelled `sdk-probe` referenced as
// `needs.sdk-probe`, which the Actions expression grammar rejects because a hyphen is
// not a dotted property access. The third class is D038's, and it is the expensive one:
// a plain `run:` scalar ending in `::` makes the whole file unreadable as YAML, GitHub
// creates no jobs, and 29 of 49 runs died in 0 s with nothing to read. Leg 7 is that
// fault. A YAML parser would have caught the first and third and said nothing about the
// second, and the silent class is the worse one: an `if:` that typos an output name does
// not fail, it evaluates to empty and skips its leg forever.
//
// Every text leg is therefore driven against a mutant of this same file, below, and the
// battery asserts which leg fires; leg 8 is a property of the directory, so it has its own
// six discovery cases. A leg that stops firing reddens this script.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// D041's residual (1) was that this script opened one named file, so a workflow added
// beside `ci.yml` went unchecked by construction — and the fault class it audits is
// per-file: a second bad file creates zero jobs just like the first one did. The default
// is therefore the directory. Leg 8 fires when the scan finds no workflow in it, because
// a gate with nothing to read must not be able to print OK. The extension test is
// case-insensitive on purpose: over-covering a file GitHub never runs is harmless, and
// under-covering one it does run is the hole.
const DIR = fileURLToPath(new URL("workflows/", import.meta.url));
const NAMED = process.argv.slice(2);

function discover(names) {
  const wf = (n) => /\.ya?ml$/i.test(n);
  return { checked: names.filter(wf).sort(), ignored: names.filter((n) => !wf(n)).sort() };
}

const KEY = /^([A-Za-z0-9_-]+):(?:[ ]*($|#.*|[^\s#].*))?$/;
const BLOCK = new Set(["|", "|-", "|+", ">", ">-", ">+"]);

function indentOf(s) { return s.length - s.trimStart().length; }
function isSkip(s) { const t = s.trim(); return t === "" || t.startsWith("#"); }
function unquote(s) { return s.length > 1 && s.startsWith('"') && s.endsWith('"') ? s.slice(1, -1) : s; }
function isQuoted(s) { return s.length > 1 && ((s.startsWith('"') && s.endsWith('"')) || (s.startsWith("'") && s.endsWith("'"))); }

// Leg 7 reads these, so the recording happens where a value is still a source string:
// after unquoting, `run: "… ndi::"` and `run: … ndi::` are the same bytes.
function notePlain(st, raw, where, line) {
  if (raw === "" || BLOCK.has(raw) || isQuoted(raw)) return;
  st.plain.push({ raw, where, line });
}

function parseMap(lines, i, ind, where, st) {
  const map = {};
  while (i < lines.length) {
    const line = lines[i];
    if (isSkip(line)) { i += 1; continue; }
    const cur = indentOf(line);
    if (cur < ind) break;
    if (cur > ind) {
      st.errs.push(`${where}: line ${i + 1} is indented ${cur}, deeper than its mapping at ${ind}`);
      return [map, i + 1];
    }
    if (line.slice(ind).startsWith("- ")) break;
    const m = KEY.exec(line.slice(ind));
    if (!m) {
      st.errs.push(`${where}: line ${i + 1} is not a mapping key: ${JSON.stringify(line.trim())}`);
      i += 1;
      continue;
    }
    const key = m[1];
    const val = (m[2] || "").replace(/ #.*$/, "").trim();
    notePlain(st, val, `${where}.${key}`, i + 1);
    if (Object.prototype.hasOwnProperty.call(map, key)) {
      st.dups.push(`${where}.${key}: duplicate mapping key, lines ${map[key].line} and ${i + 1}`);
    }
    const child = parseValue(lines, i, ind, val, `${where}.${key}`, st);
    map[key] = { value: child.value, line: i + 1 };
    i = child.next;
  }
  return [map, i];
}

function parseSeq(lines, i, ind, where, st) {
  const seq = [];
  while (i < lines.length) {
    const line = lines[i];
    if (isSkip(line)) { i += 1; continue; }
    const cur = indentOf(line);
    if (cur < ind || !line.slice(cur).startsWith("- ")) break;
    if (cur !== ind) st.errs.push(`${where}: sequence item on line ${i + 1} is at ${cur}, expected ${ind}`);
    const rest = line.slice(cur).replace(/^- +/, "");
    const col = cur + (line.slice(cur).length - rest.length);
    const m = KEY.exec(rest);
    if (!m) {
      const scalar = rest.replace(/ #.*$/, "").trim();
      notePlain(st, scalar, `${where}[${i + 1}]`, i + 1);
      seq.push({ scalar, line: i + 1 });
      i += 1;
      continue;
    }
    const key = m[1];
    const val = (m[2] || "").replace(/ #.*$/, "").trim();
    notePlain(st, val, `${where}[${i + 1}].${key}`, i + 1);
    const child = parseValue(lines, i, col, val, `${where}[${i + 1}]`, st);
    const item = { [key]: { value: child.value, line: i + 1 } };
    const [more, next] = parseMap(lines, child.next, col, `${where}[${i + 1}]`, st);
    seq.push(Object.assign(item, more));
    i = next;
  }
  return [seq, i];
}

function parseValue(lines, i, ind, val, where, st) {
  if (BLOCK.has(val)) {
    const body = [];
    let j = i + 1;
    while (j < lines.length) {
      if (lines[j].trim() !== "" && indentOf(lines[j]) <= ind) break;
      body.push(lines[j]);
      j += 1;
    }
    return { value: body.join("\n"), next: j };
  }
  if (val !== "") return { value: unquote(val), next: i + 1 };
  let j = i + 1;
  while (j < lines.length && isSkip(lines[j])) j += 1;
  if (j >= lines.length) return { value: null, next: j };
  const cur = indentOf(lines[j]);
  if (cur <= ind) return { value: null, next: j };
  if (lines[j].slice(cur).startsWith("- ")) {
    const [seq, next] = parseSeq(lines, j, cur, where, st);
    return { value: seq, next };
  }
  const [map, next] = parseMap(lines, j, cur, where, st);
  return { value: map, next };
}

const get = (node, key) => (node && typeof node === "object" && node[key] !== undefined ? node[key].value : undefined);
const deref = (w) => (w && typeof w === "object" && w.value !== undefined ? w.value : w);
const entries = (node) => (node && typeof node === "object" && !Array.isArray(node) ? Object.entries(node) : []);

function audit(text) {
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  const st = { errs: [], dups: [], plain: [] };
  const faults = [];
  const legs = [];
  const fire = (leg, msg) => faults.push({ leg, msg });

  const [root] = parseMap(lines, 0, 0, "<root>", st);
  if (st.errs.length) st.errs.forEach((m) => fire(0, m));
  else legs.push([0, `block mapping over ${lines.length} lines, block scalars skipped`]);
  if (st.dups.length) st.dups.forEach((m) => fire(1, m));
  else legs.push([1, "no mapping scope holds the same key twice"]);

  const jobList = entries(get(root, "jobs")).map(([id, w]) => ({ id, map: deref(w) }));
  if (!jobList.length) fire(3, "jobs: no jobs block or no jobs declared");

  const refs = [];
  const walk = (node, job) => {
    if (Array.isArray(node)) return node.forEach((x) => walk(x, job));
    if (node && typeof node === "object") {
      Object.entries(node).forEach(([, v]) => walk(v.value, job));
      return;
    }
    if (typeof node !== "string") return;
    for (const m of node.matchAll(/needs\.[A-Za-z0-9_-]+(?:\.outputs\.[A-Za-z0-9_-]+)?/g)) {
      refs.push({ job, text: m[0] });
    }
    for (const m of node.matchAll(/steps\.([A-Za-z0-9_-]+)\.outputs\.([A-Za-z0-9_-]+)/g)) {
      refs.push({ job, stepId: m[1], output: m[2], text: m[0] });
    }
  };
  for (const j of jobList) walk(j.map, j.id);

  const needsRefs = refs.filter((r) => r.text.startsWith("needs.") && !r.stepId);
  let n2 = 0;
  for (const r of needsRefs) {
    const name = r.text.split(".")[1];
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) {
      fire(2, `${name} in ${r.text} (job "${r.job}"): an Actions expression takes a dotted identifier, and a hyphen is not one`);
      n2 += 1;
    }
  }
  if (!n2) legs.push([2, `${refs.length} expression references, every needs-dot target an identifier`]);

  const declared = new Set(jobList.map((j) => j.id));
  const byId = new Map(jobList.map((j) => [j.id, j.map]));
  const needsOf = (job) => {
    const n = get(job, "needs");
    if (n === undefined || n === null) return [];
    if (Array.isArray(n)) return n.map((x) => x.scalar || x);
    return [n];
  };
  let n3 = 0;
  for (const { id, map } of jobList) {
    for (const name of needsOf(map)) {
      if (!declared.has(name)) {
        fire(3, `job "${id}" has needs: "${name}", which is not a declared job (jobs: ${[...declared].join(", ")})`);
        n3 += 1;
      }
    }
  }
  for (const r of needsRefs) {
    if (!declared.has(r.text.split(".")[1])) {
      fire(3, `${r.text} in job "${r.job}" names a job this workflow does not declare`);
      n3 += 1;
    }
  }
  if (!n3) legs.push([3, `${declared.size} jobs, every needs target declared`]);

  let n4 = 0;
  for (const { id: jobId, map: job } of jobList) {
    const needs = needsOf(job);
    for (const r of needsRefs.filter((x) => x.job === jobId)) {
      const parts = r.text.split(".");
      if (parts.length < 4 || parts[2] !== "outputs") continue;
      const target = parts[1];
      const key = parts[3];
      if (!needs.includes(target)) {
        fire(4, `job "${jobId}" uses ${r.text} but its needs is [${needs.join(", ")}]`);
        n4 += 1;
        continue;
      }
      if (get(byId.get(target), "outputs")?.[key] === undefined) {
        fire(4, `job "${jobId}" uses ${r.text}, but job "${target}" declares no output "${key}"`);
        n4 += 1;
      }
    }
  }
  if (!n4) legs.push([4, "every needs-dot-outputs key declared by the job it names"]);

  let n5 = 0;
  for (const { id: jobId, map: job } of jobList) {
    const steps = get(job, "steps") || [];
    const ids = new Set(steps.filter((s) => s.id).map((s) => s.id.value));
    for (const [outName, out] of entries(get(job, "outputs") || {})) {
      for (const m of String(out.value).matchAll(/steps\.([A-Za-z0-9_-]+)\.outputs\.([A-Za-z0-9_-]+)/g)) {
        if (!ids.has(m[1])) {
          fire(5, `job "${jobId}" output "${outName}" reads ${m[0]}, and no step in that job has id "${m[1]}"`);
          n5 += 1;
          continue;
        }
        const stepText = JSON.stringify(steps.find((s) => s.id && s.id.value === m[1]));
        // The step must name the key where it writes it, not merely mention it: a print
        // line that quotes the name would satisfy a looser scan and the output would
        // still be empty. The shape accepted is the one this workflow uses — a
        // PowerShell table entry `k = 'name'` feeding Add-Content $env:GITHUB_OUTPUT.
        // `echo "name=$v"` is deliberately NOT accepted, because nothing here writes that
        // way and an unexercised alternative is a clause no control can prove; if a leg
        // ever goes red on that shape, this is the line to widen.
        if (!new RegExp(`k\\s*=\\s*'${m[2]}'`).test(stepText)) {
          fire(5, `job "${jobId}" reads ${m[0]}, but step "${m[1]}" never writes "${m[2]}" as a GITHUB_OUTPUT table entry`);
          n5 += 1;
        }
      }
    }
  }
  if (!n5) legs.push([5, "every job output is written by a step that exists and writes it"]);

  let n6 = 0;
  let stepCount = 0;
  for (const { id: jobId, map: job } of jobList) {
    for (const key of ["timeout-minutes", "runs-on"]) {
      if (get(job, key) === undefined) {
        fire(6, `job "${jobId}" declares no ${key}`);
        n6 += 1;
      }
    }
    for (const s of get(job, "steps") || []) {
      stepCount += 1;
      if (!s.uses && !s.run) {
        fire(6, `job "${jobId}" step ${s.name ? `"${s.name.value}"` : "(unnamed)"} has neither uses: nor run:`);
        n6 += 1;
      }
    }
  }
  if (!n6) legs.push([6, `${declared.size} jobs / ${stepCount} steps all bounded, based and executable`]);

  // D038's fault class. PyYAML reads a plain scalar's trailing colon as a mapping
  // indicator and raises `mapping values are not allowed here`, which is why run
  // 44189589142 has an empty `/jobs` array. Quoting the value fixes it, so this leg is
  // only about *unquoted* scalars — `plainScalarColon` below unquotes the one line in
  // this file that ends in `::` and requires leg 7, and only leg 7, to fire on it.
  let n7 = 0;
  for (const s of st.plain) {
    const shape = /:$/.test(s.raw) ? "ends with a colon" : /: /.test(s.raw) ? "holds a colon-space" : null;
    if (shape) {
      fire(7, `${s.where} (line ${s.line}): the plain scalar ${shape}, so YAML reads it as a mapping indicator and the file yields no jobs — quote it: ${JSON.stringify(s.raw)}`);
      n7 += 1;
    }
  }
  if (!n7) legs.push([7, `${st.plain.length} plain scalars, none in the shape that makes YAML raise`]);

  return { faults, legs, census: `${declared.size} jobs, ${stepCount} steps, ${refs.length} references, ${st.plain.length} plain scalars` };
}

// Each mutant names the legs that must fire on it and, by the equality check below,
// which must stay clean. `inScalarControl` is the anti-false-positive leg: a duplicated
// assignment inside a `run: |` body is PowerShell, not YAML, and must not be reported.
const MUTANTS = [
  { name: "control", from: null, expect: [] },
  { name: "dupShellKey", from: "        id: probe\n", to: "        id: probe\n        shell: pwsh\n", expect: [1] },
  { name: "hyphenInNeedsRef", from: "if: needs.sdk_probe.outputs.link == 'true'", to: "if: needs.sdk-probe.outputs.link == 'true'", expect: [2, 3, 4] },
  { name: "undeclaredNeedsJob", from: "    needs: sdk_probe\n    if: needs.sdk_probe.outputs.link", to: "    needs: sdk_probe_x\n    if: needs.sdk_probe.outputs.link", expect: [3, 4] },
  { name: "typoOutputKey", from: "needs.sdk_probe.outputs.spout == 'true'", to: "needs.sdk_probe.outputs.spout2 == 'true'", expect: [4] },
  { name: "unknownStepId", from: "      link: ${{ steps.probe.outputs.link }}", to: "      link: ${{ steps.probez.outputs.link }}", expect: [5] },
  { name: "outputNeverNamed", from: "@{ k = 'vcvars'; v = $vcvars }", to: "@{ k = 'vcvarsX'; v = $vcvars }", expect: [5] },
  // The 75 is this file's own, and only verify carries it: if the bound moves, this
  // control reports "matches 0 times" and names the line to edit.
  { name: "unboundedJob", from: "    timeout-minutes: 75\n", to: "", expect: [6] },
  { name: "overIndentedKey", from: "        run: cargo fmt --all -- --check", to: "      run: cargo fmt --all -- --check", expect: [0, 6] },
  { name: "plainScalarColon", from: 'run: "cargo test -p flux-integrations --features ndi ndi::"', to: "run: cargo test -p flux-integrations --features ndi ndi::", expect: [7] },
  { name: "inScalarControl", from: "                $spout = 'true'\n", to: "                $spout = 'true'\n                $spout = 'true'\n", expect: [] },
];

// Leg 8 has no text anchor — it is a property of the directory, not of a file — so it is
// driven against the discovery function directly. `emptyDir` and `notesOnly` are its
// must-fire witnesses; the rest prove it does not fire on the shapes this repo can
// actually end up with, including a second workflow file arriving.
const DISCOVER_CASES = [
  { name: "emptyDir", names: [], want: { checked: 0, ignored: 0, leg8: true } },
  { name: "onlyCi", names: ["ci.yml"], want: { checked: 1, ignored: 0, leg8: false } },
  { name: "secondWorkflow", names: ["ci.yml", "release.yaml"], want: { checked: 2, ignored: 0, leg8: false } },
  { name: "nonWorkflowIgnored", names: ["ci.yml", "README.md"], want: { checked: 1, ignored: 1, leg8: false } },
  { name: "upperCaseExtension", names: ["CI.YML"], want: { checked: 1, ignored: 0, leg8: false } },
  { name: "notesOnly", names: ["notes.md"], want: { checked: 0, ignored: 1, leg8: true } },
];

// core.autocrlf gives these files CRLF on Windows and LF elsewhere; the mutants below are
// written against LF, so the text they patch is normalized first.
const DISC = discover(NAMED.length ? NAMED.map((f) => path.basename(f)) : fs.readdirSync(DIR));
const FILES = NAMED.length ? NAMED.map((f) => path.resolve(f)) : DISC.checked.map((n) => path.join(DIR, n));
const READS = FILES.map((f) => ({
  name: path.basename(f),
  source: fs.readFileSync(f, "utf8").replace(/\r\n/g, "\n"),
}));

const legs = [];
const faults = [];
if (DISC.checked.length) {
  legs.push([8, `${DISC.checked.length} workflow file(s) audited from .github/workflows${
    DISC.ignored.length ? `; ${DISC.ignored.length} non-YAML file(s) left alone (${DISC.ignored.join(", ")})` : ""}`]);
} else {
  faults.push({ leg: 8, msg: `no *.yml / *.yaml under ${DIR} — this gate has nothing to read, which is not a pass` });
}
for (const r of READS) {
  const a = audit(r.source);
  r.census = a.census;
  for (const [n, detail] of a.legs) legs.push([n, `${detail} [${r.name}]`]);
  for (const f of a.faults) faults.push({ leg: f.leg, msg: `${f.msg} [${r.name}]` });
}
for (const [n, detail] of legs.sort((x, y) => x[0] - y[0])) console.log(`leg ${n}: OK (${detail})`);
for (const f of faults) console.error(`FAIL leg ${f.leg} — ${f.msg}`);

let controlFaults = 0;
// The text mutants are ci.yml's own bytes. If ci.yml is not being audited they would all
// report "matches 0 times", so say that plainly once instead of eleven times — and never
// skip the battery, which is the vacuous-green this whole file exists to refuse.
const HOST = (READS.find((r) => r.name === "ci.yml") || READS[0]);
if (!HOST) {
  console.error("FAIL controls: no audited file to mutate, so the text battery did not run");
  controlFaults += 1;
} else {
  for (const m of MUTANTS) {
    let text = HOST.source;
    if (m.from !== null) {
      const hits = HOST.source.split(m.from).length - 1;
      if (hits !== 1) {
        console.error(`FAIL control ${m.name}: its \`from\` text matches ${hits} times in ${HOST.name}, so the mutant is not the file`);
        controlFaults += 1;
        continue;
      }
      text = HOST.source.replace(m.from, m.to);
    }
    const got = [...new Set(audit(text).faults.map((f) => f.leg))].sort();
    const want = [...m.expect].sort();
    if (got.join(",") !== want.join(",")) {
      console.error(`FAIL control ${m.name}: expected legs [${want.join(",") || "clean"}], got [${got.join(",") || "clean"}]`);
      controlFaults += 1;
      continue;
    }
    console.log(`control ${m.name}: ${want.length ? `legs ${want.join(",")} fired` : "clean"} as required`);
  }
}

for (const c of DISCOVER_CASES) {
  const got = discover(c.names);
  const want = `${c.want.checked} checked, ${c.want.ignored} ignored, leg 8 ${c.want.leg8 ? "fires" : "clean"}`;
  const have = `${got.checked.length} checked, ${got.ignored.length} ignored, leg 8 ${got.checked.length ? "clean" : "fires"}`;
  if (want !== have) {
    console.error(`FAIL discover ${c.name}: expected [${want}], got [${have}]`);
    controlFaults += 1;
    continue;
  }
  console.log(`discover ${c.name}: ${have} as required`);
}

const controlTotal = MUTANTS.length + DISCOVER_CASES.length;
if (faults.length || controlFaults) {
  console.error(`workflow check: ${faults.length} fault(s) across ${READS.length} file(s), ${controlFaults} control fault(s)`);
  process.exit(1);
}
console.log(`workflow check: ${READS.map((r) => `${r.name} clean (${r.census})`).join(", ")}; ${controlTotal} controls as required`);
