# Flux WebGPU shader conformance harness

This static page loads the catalog's generated WGSL (`shaders.json`, emitted by
`cargo run -p xtask --bin export_shaders`) and compiles each shader module on the
browser's WebGPU device. It is a conformance/tooling target for the portable shader
ABI — it does not run a second simulation and is not an output path.

## Generate the shader bundle
```
cargo run -p xtask --bin export_shaders
```
This also Naga-validates every shader (same validator as the native renderer) and
fails if any shader is invalid.

## Run
Serve the directory over HTTP (WebGPU requires a secure context; `localhost` is
allowed) and open `index.html`:
```
node serve.js
# then open http://localhost:8099/index.html in Chrome/Edge (WebGPU enabled)
```

If WebGPU is unavailable, the page reports that state explicitly — it never claims
a pass without a real device. Native GPU compilation (`cargo xtask test-gpu`) remains
the authoritative shader gate.
