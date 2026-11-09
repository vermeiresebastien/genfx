// Read-only validation of this planning package. This does not test the application.
import { readFile, readdir } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(fileURLToPath(import.meta.url));
const files = (await readdir(root)).filter((name) => name.endsWith('.md')).sort();
const docs = Object.fromEntries(await Promise.all(files.map(async (name) => [name, await readFile(join(root, name), 'utf8')])));
const requiredFiles = ['START-HERE.md','AGENT-PROMPT.md','ARCHITECTURE.md','CONTRACTS.md','EFFECTS.md','GENERATORS.md','TASKS.md','VERIFICATION.md','REPOSITORIES.md'];
const failures = [];
for (const name of requiredFiles) if (!docs[name]) failures.push(`Missing file: ${name}`);

const select = (file, regex) => [...(docs[file] ?? '').matchAll(regex)].map((match) => ({id:match[1],title:match[2]?.trim()}));
const sections = {
  product: select('START-HERE.md', /^\| (R\d{3}) \| ([^|]+)/gm),
  effects: select('EFFECTS.md', /^### (FX\d{3}) — (.+?) —/gm),
  effectFamilies: select('EFFECTS.md', /^\| (FX\d{3}) ([^|]+)/gm),
  generators: select('GENERATORS.md', /^\| (G\d{3}) \| ([^|]+)/gm),
  tasks: select('TASKS.md', /^### (T\d{2}) — (.+)/gm),
  acceptance: select('VERIFICATION.md', /^\| (AT\d{3}) \| ([^|]+)/gm)
};
const expectedCounts = {product:24,effects:28,effectFamilies:24,tasks:28,acceptance:85};
for (const [key, records] of Object.entries(sections)) {
  if (new Set(records.map((r)=>r.id)).size !== records.length) failures.push(`Duplicate IDs: ${key}`);
  if (key in expectedCounts && records.length !== expectedCounts[key]) failures.push(`${key} expected ${expectedCounts[key]}, found ${records.length}`);
}
const known = new Set(Object.values(sections).flat().map((record)=>record.id));
for (const [file, content] of Object.entries(docs)) {
  const fences = content.split('\n').filter((line)=>/^```/.test(line)).length;
  if (fences % 2 !== 0) failures.push(`Unbalanced code fences: ${file}`);
  for (const match of content.matchAll(/\b(?:AT\d{3}|FX\d{3}|G\d{3}|R\d{3}|T\d{2})\b/g)) {
    if (!known.has(match[0])) failures.push(`Undefined ID ${match[0]} in ${file}`);
  }
  for (const match of content.matchAll(/`([A-Z][A-Z-]*\.md)`/g)) {
    if (!docs[match[1]] && !['AGENTS.md','DECISIONS.md','BLOCKERS.md','FINAL-REPORT.md','THIRD_PARTY_NOTICES.md','README.md'].includes(match[1])) failures.push(`Missing referenced spec ${match[1]} in ${file}`);
  }
  for (const match of content.matchAll(/```json\s*\n([\s\S]*?)\n```/g)) {
    try { JSON.parse(match[1]); } catch (error) { failures.push(`Invalid JSON example in ${file}: ${error.message}`); }
  }
  for (const match of content.matchAll(/\]\(([^)]+)\)/g)) {
    const target = match[1].replace(/^<|>$/g, '').split('#')[0];
    if (!target || /^[a-z]+:/i.test(target)) continue;
    try { await readFile(join(root,decodeURIComponent(target))); }
    catch { failures.push(`Broken relative link ${target} in ${file}`); }
  }
}
const taskText=docs['TASKS.md']??'';
const testedIds=new Set([...taskText.matchAll(/\bAT\d{3}\b/g)].map((m)=>m[0]));
for(const match of taskText.matchAll(/AT(\d{3})[–-]AT(\d{3})/g)) {
  for(let n=Number(match[1]);n<=Number(match[2]);n++) testedIds.add(`AT${String(n).padStart(3,'0')}`);
}
for(const row of sections.acceptance) if(!testedIds.has(row.id)) failures.push(`No task gate references ${row.id}`);
try {
  const savedIndex=JSON.parse(await readFile(join(root,'SPEC-INDEX.json'),'utf8'));
  for(const [key,rows] of Object.entries(sections)) {
    const indexRows=savedIndex.requirements[key];
    if(!indexRows||indexRows.map((r)=>r.id).join(',')!==rows.map((r)=>r.id).join(',')) failures.push(`Index drift: ${key}`);
  }
} catch(error) { failures.push(`Invalid/missing SPEC-INDEX.json: ${error.message}`); }
const result = {
  scope:'Specification structure only; no application build or runtime verification',
  status:failures.length?'failed':'passed',
  files:files.length,
  counts:Object.fromEntries(Object.entries(sections).map(([key,rows])=>[key,rows.length])),
  wordCount:Object.values(docs).join('\n').split(/\s+/).filter(Boolean).length,
  failures:[...new Set(failures)]
};
if (process.argv.includes('--index')) result.requirements=sections;
process.stdout.write(JSON.stringify(result,null,2)+'\n');
if (failures.length) process.exitCode=1;
