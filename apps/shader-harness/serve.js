// Minimal static file server for the WebGPU harness (localhost is a secure
// context, so WebGPU is permitted). No dependencies.
const http = require("http");
const fs = require("fs");
const path = require("path");

const PORT = 8099;
const ROOT = __dirname;
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".json": "application/json" };

http
  .createServer((req, res) => {
    const rel = req.url === "/" ? "/index.html" : req.url.split("?")[0];
    const file = path.join(ROOT, path.normalize(rel).replace(/^(\.\.[/\\])+/, ""));
    fs.readFile(file, (err, data) => {
      if (err) { res.writeHead(404); res.end("not found"); return; }
      res.writeHead(200, { "content-type": TYPES[path.extname(file)] || "application/octet-stream" });
      res.end(data);
    });
  })
  .listen(PORT, () => console.log(`shader harness: http://localhost:${PORT}/index.html`));
