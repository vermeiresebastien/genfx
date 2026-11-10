// Rebuilds apps/desktop/ui/app.bundle.js from the ES-module entry (main.js)
// and rewrites the version query on the script URL with a content hash, so a
// new bundle is never served from a stale WebView2 cache.
//
// Usage: node bundle.mjs
import { build } from "esbuild";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const bundlePath = join(here, "app.bundle.js");
const indexPath = join(here, "index.html");

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

// Content-addressed version token: a changed bundle produces a new URL, which
// defeats WebView2's HTTP cache. This is the fix for "old number-input inspector
// kept showing up".
const hash = createHash("sha256").update(readFileSync(bundlePath)).digest("hex").slice(0, 12);

let html = readFileSync(indexPath, "utf8");
html = html.replace(
  /src="app\.bundle(?:\.js|\.[0-9a-f]{12}\.js)\?v=[^"]*"/,
  `src="app.bundle.js?v=${hash}"`
);
writeFileSync(indexPath, html);

console.log(`bundled app.bundle.js?v=${hash}`);
