# Effect and fill implementation catalog

All catalog names describe visual functions. Do not include creator names, attribution columns, or personal names in this catalog or the effect UI. Library license notices are maintained separately for distribution.

## 1. Common implementation requirements

Every effect has manifest version 1, stable ID, help text, actual implementation, deterministic fixture, non-default parameter test, wet/dry test, transparency test, resize test, and a preset demonstrating it. Numeric ranges below are native units; mapping uses ARCHITECTURE. All parameters accept keyframes/modulation except explicit resource/enum controls. Default wet=1, enabled=true, mask=white, seed inherited. Motion speed is cycles/s or pixels/s, never per frame. Spatial sizes refer to output pixels at project resolution and compensate preview scale.

Effects accept Texture2D `source` and optional Mask `mask`. Additional inputs are explicitly listed. Fills are Generator nodes with no required source, RGBA output, optional field/depth outputs, finite timeline duration, and transparency control. A graph template using shared primitives is acceptable only when the named node remains discoverable, versioned, editable, and behaviorally distinct. Do not make different names aliases for the same identity shader.

The final effect mix is `lerp(source, processed, wet*mask)` in premultiplied linear color. Source-independent fills mix against transparent or a user-selected background. Border modes default clamp; warp/refraction exposes mirror/repeat/transparent. Document whether an effect deliberately processes display-referred RGB or linear RGB. Denominators have epsilon guards; empty textures and alpha=0 never yield NaNs.

Quality classes: L = one/few fullscreen passes; M = multipass or moderate samples; H = large samples/history/compute. These are estimates; actual GPU timings appear in diagnostics.

## 2. The exact 28 requested entries

### FX001 — Bloom — `fx.bloom` — M

Extract bright linear RGB with threshold=1 [0,8], soft_knee=0.5 [0,1]. Build downsample pyramid up to 5 levels, separable tent/Gaussian blur and upsample sum. radius=24 px [0,256], intensity=0.6 [0,4], tint=white. Composite additive radiance with an explicit alpha policy: preserve source alpha by default; `expand_alpha` optional creates visible glow outside silhouette. Threshold 0 affects all lit pixels, radius 0 is well-defined. Test impulse radial symmetry, HDR highlight response, black invariance, and stable brightness across preview scales.

### FX002 — Bokeh blur — `fx.bokeh_blur` — H

Optional Depth input; otherwise uniform aperture blur. radius=12 px [0,96], blades=6 [3,12], rotation=0 rad [0,2π], highlight_boost=0 [0,4], focus_distance=3 m, focus_range=1 m. Gather normalized aperture samples using deterministic disk/polygon distribution (quality 16/32/64 taps). Premultiplied sampling prevents dark edges. Depth mode includes foreground/background separation or edge-aware rejection to reduce foreground bleed; document approximate single-layer depth limits. Test point highlight aperture shape, constant-color preservation, and zero-radius identity.

### FX003 — Channel mixer — `fx.channel_mixer` — L

Apply 3×3 RGB matrix and RGB offset to unpremultiplied working RGB, preserve alpha. Defaults identity matrix, offsets 0 [-2,2], coefficients [-2,2], monochrome=false. Monochrome uses configurable weights. Output retains HDR headroom until final transform. Test channel swap and explicit matrix multiplication against CPU oracle.

### FX004 — Chromatic metal — `fx.chromatic_metal` — M

Derive pseudo-normal from source luminance gradient or optional Normal input; reflect a procedural environment with spectral/rainbow bands and Fresnel rim. emboss=4 px [0,32], roughness=0.25 [0,1], metallic=1 [0,1], hue_shift=0, light_angle=0.7 rad, environment_speed=0.1 cycles/s. This is a stylized material on a 2D relief; no fake claim of reconstructed 3D. Test normal/light direction change and distinct color bands at nonzero strength.

### FX005 — Color adjustment — `fx.color_adjustment` — L

Exposure=0 EV [-10,10], contrast=1 [0,4] around linear 18% gray, saturation=1 [0,3], hue=0 deg [-180,180], temperature=0 [-1,1], tint=0 [-1,1], lift=0 [-1,1], gamma=1 [0.1,5], gain=1 [0,4]. Define order exposure→white balance→lift/gamma/gain→contrast→hue/saturation. All neutral controls identity within tolerance. Test grayscale remains neutral, exposure +1 doubles linear RGB before clipping, alpha unchanged.

### FX006 — Colored edges — `fx.colored_edges` — M

Sobel luminance gradient, threshold=0.15 [0,2], softness=0.05, thickness=1 px [0.5,12], edge_color=cyan, background_mix=0 [0,1], hue_by_direction=false. Thicker edges use dilation of edge mask. Output `edges` mask additionally. Test constant image has no edges, straight boundary localized, direction mode changes hue consistently.

### FX007 — Dither — `fx.dither` — L/H by mode

Modes Bayer 2/4/8, blue-noise, monochrome random, Floyd–Steinberg, Sierra. Default Bayer4, levels=2 [2,32], scale=1 px [1,16], monochrome=true, phase_animation=false. Quantize perceptual RGB/luma explicitly; restore working linear output. Error diffusion uses deterministic scan order or validated wavefront algorithm; do not pretend independent fragments implement serial diffusion. Native CPU diffusion at reduced preview size and offline full resolution is acceptable and shows H cost. Test palette membership, fixed-seed determinism, mean-luma preservation within tolerance, mode-specific pattern.

### FX008 — Filter presets — `preset.color_filter` — L

Preset bank built from color adjustment, channel mixer, curves, gradient maps, grain, and vignette: Neutral, Warm, Cool, Monochrome, High Contrast, Faded Film, Duotone, Solarized, Thermal. intensity=1 [0,1]. Neutral exact identity; each other preset has documented coefficients/LUT and distinct fixture. Users can expand a preset to editable nodes. A combo box without actual transform is incomplete.

### FX009 — Gooey merge — `fx.gooey_merge` — M

Optional second image or mask. Composite inputs into alpha field, blur field radius=12 px [0,96], threshold=0.5 [0,1], edge_softness=0.05 [0.001,0.5], preserve_color=true, fill_color=white. Reconstruct smooth merged silhouette using smoothstep of blurred alpha; resolve source color by normalized blurred premultiplied RGB to avoid black fringes. Outputs color and mask. Two nearby circles merge at configured distance; far circles remain distinct. Empty input transparent; preserve-color and fill modes both tested.

### FX010 — Gradient map — `fx.gradient_map` — L

Map luminance to editable sorted gradient stops, minimum 2 maximum 32. Default black→white, input_black=0, input_white=1, interpolation=linear RGB, reverse=false. Handle equal input bounds with defined threshold. Option alpha from gradient=false. Test exact endpoints, middle interpolation, stop edits, and source alpha preservation.

### FX011 — Halftone — `fx.halftone` — L/M

Cell size=8 px [2,128], angle=45 deg, dot shape circle/ellipse/square/line, ink=black, paper=white, contrast=1, CMYK=false. Derive area coverage from cell luma; antialias via derivatives. CMYK uses per-channel screen angles and ink overlap; not four identical rotations. Outputs ink mask. Test coverage at white/black/gray, rotation, and scale consistency.

### FX012 — Hatching — `fx.hatching` — M

Spacing=8 px [2,128], width=1 px [0.25,8], angle=45 deg, cross_hatch=true, tone_layers=3 [1,6], ink/paper colors, wobble=0 [0,8 px]. Add line layers at luminance thresholds, deterministic seeded jitter. Test dark region has more line coverage, angle changes orientation, flat white remains paper.

### FX013 — Lens distortion — `fx.lens_distortion` — L

Normalized centered radius with aspect correction. radial k1=0 [-1,1], k2=0 [-1,1], tangential p1/p2=0 [-0.2,0.2], center=(0.5,0.5), zoom=1 [0.1,4], border=transparent. Inverse-map output to source; bounded iterations if solving inverse lens model. Test identity, barrel/pincushion direction, center invariance, rectangular aspect.

### FX014 — Outlines — `fx.outlines` — M

Input source or Mask; mode silhouette/luminance/normal-depth; thickness=2 px [0.5,32], threshold=0.2, color=white, inner=false, outer=true, source_mix=1. Silhouette uses distance/dilate-erode, luminance uses edges, geometry route compares actual depth/normals. Test transparent shape border, thickness, and no outlines in uniform field.

### FX015 — Pattern refraction — `fx.pattern_refraction` — M

Aux texture/VectorField or built-in stripes/grid/cellular/waves. Treat pattern as height h; finite gradient bends UV by strength=12 px [0,256]. scale=32 px [2,1024], rotation=0, IOR=1.45 [1,2.5], chromatic_split=0 [0,12 px], speed=0.1, border=mirror. Can output displacement field. Test zero strength identity and stripe orientation changes bend direction.

### FX016 — Pixel stretch — `fx.pixel_stretch` — L

Axis horizontal/vertical/radial, origin=0.5, source_band=1 px [1,256], extent=0.3 [0,1], direction=positive/negative/both, feather=0. Test source strip extends without out-of-range sampling and extent=0 identity. Expose animated origin for scan effects.

### FX017 — Pixelate — `fx.pixelate` — L

block_size=(8,8) px [1,512], shape square/hex, sampling center/average, grid_offset=(0,0), aspect_lock=true. Square center uses quantized UV; average uses bounded downsample, not arbitrary huge loops. Optional palette quantization is separate parameter/node. Test block-size=1 near identity, constant colors and rectangular blocks.

### FX018 — Slice shift — `fx.slice_shift` — L

Axis horizontal/vertical, slices=32 [1,512], displacement=20 px [0,512], active_probability=0.2 [0,1], update_rate=8 Hz [0,120], seed. Random values held for deterministic time cells, optional beat-trigger hold, wrap mode explicit. Test within-slice consistency, seed repeatability, probability=0 identity, and frame-rate-independent update rate.

### FX019 — Warp — `fx.warp` — L/M

Aux VectorField or noise field. amount=24 px [0,512], frequency=3 [0.01,64], speed=0.2, octaves=3 [1,8], axis weights=(1,1), border=mirror. Normalize field units and sample source backward. Expose field-only output. Test zero amount identity, known constant-vector translation, seeded time motion.

### FX020 — Clouds — `gen.clouds` — M

2D fBm density, scale=3, octaves=5 [1,8], coverage=0.5, softness=0.2, speed=(0.02,0), foreground/background colors, alpha_density=false. Domain-warp optional. Volumetric clouds are a separate 3D variant. Test deterministic density, coverage monotonicity, and moving field at explicit time.

### FX021 — Concentric patterns — `gen.concentric` — L

Shapes circle/square/polygon, count=12 [1,256], spacing=32 px, thickness=4 px, center=(0.5,0.5), phase=0 cycles, speed=0.1 cycles/s, twist=0. Antialias; gradient palette optional. Test center alignment, ring count, changing phase, non-square aspect.

### FX022 — Fractal noise — `gen.fractal_noise` — M

Types value/gradient/simplex-compatible/cellular; fBm octaves=5, lacunarity=2 [1,4], persistence=0.5 [0,1], scale=4, speed=0.1, seed, seamless=false. Implement original/permissively licensed kernels; no assumed automatic GLSL translation. Seamless mode periodic coordinates and verified seams. Output color + scalar field/height. Test repeatability and periodic border equality.

### FX023 — Glowing wave — `gen.glowing_wave` — M

Curve sine/sum-of-sines/path, amplitude=0.2, frequency=2, speed=0.2, thickness=3 px, glow=16 px, palette, count=1 [1,32]. Curve distance → soft emissive core + exponential falloff. Optional audio waveform/spectrum replaces height profile. Test curve position, transparent outside if requested, audio input deformation.

### FX024 — Mesh gradient — `gen.mesh_gradient` — M

Editable 2×2 default or up to 8×8 control grid with position/color, smooth interpolation, animation paths, speed=0.1, turbulence=0. Use triangulated grid or a defined weighted color field; label chosen implementation. Avoid calling an unrelated two-color linear gradient a mesh. Test moving control point changes only expected field smoothly; no cracks across cells.

### FX025 — Moiré — `gen.moire` — L

Two overlapping line/radial/grid fields with frequency1=24, frequency2=24.5 cycles/image, relative_angle=3 deg, phase=0, speed=0.1. Palette and blend multiply/difference. Analytic/derivative antialias and supersample option; intentional interference must remain visible at reasonable frequencies. Test envelope variation when frequency delta changes.

### FX026 — Nebula — `gen.nebula` — M/H

Layered warped fBm with colored density, sparse seeded stars, glow and depth-like parallax. scale=3, density=0.5, warp=0.4, speed=0.03, star_density=0.002, palette 3 stops. Native WGSL generated texture. Test density/stars independently and distinct output from Clouds.

### FX027 — Pattern grid — `gen.pattern_grid` — L

Cell shape square/hex/triangle/circle, rows=12, columns=20, gap=0.1, border=1 px, jitter=0, phase, fill/stroke colors, alternate pattern=false. Repetition can emit masks/instance placement descriptors through separate typed output. Test expected cells, gap, deterministic jitter and animation.

### FX028 — Water caustic — `gen.water_caustic` — M

Animated wave/cellular refractive intensity field, scale=5, speed=0.2, sharpness=8 [1,64], intensity=1, tint pale cyan, depth=1. Optional actual displaced-water normal input. This generator approximates caustic illumination; expensive physically traced caustics are not implied. Test temporal smoothness, sharpness changes line width, constant seed repeatability.

## 3. Additional required effect families

Each semicolon-separated variant below is a distinct discoverable preset or node with functional settings. Implement shared kernels once and specify variant values in manifests. Assign IDs `FX100` onward in listed row order; child variants get stable suffix IDs, e.g. FX100.levels. Include all variants in coverage reports.

| Family/ID | Required variants | Controls/algorithm | Distinguishing test |
|---|---|---|---|
| FX100 tone | levels; curves; posterize; threshold; invert; duotone; thermal palette | levels input/output black/white and gamma; curves 2–32 points monotone/interpolated; posterize 2..64 levels | endpoints, monotonic curve, palette set |
| FX101 color | hue rotate; temperature/tint; LUT 3D; palette quantize; gamut view | LUT .cube sizes 17/33/65 validated; tetrahedral/trilinear declared; strength | neutral LUT identity; known cube |
| FX102 noise | film grain; static; paper grain; distressed ink | luma/chroma amplitude, grain size, temporal hold, seed | stable seed; variance vs amplitude |
| FX103 blur | Gaussian; directional; radial; focal; motion-vector blur | separable Gaussian normalized; 16/32 samples for radial; vector px/s × shutter fraction | impulse kernel and constant preservation |
| FX104 glow | outer glow; halo echo; light rays; light leaks; thermal glow | silhouette dilation/blur; offset repeated halo; radial occlusion rays; seeded leak layer | alpha expansion explicit; center direction |
| FX105 chromatic | chromatic split; prism split; tape split | per-channel UV offsets with border policy, mix | white edge separates into channels |
| FX106 VHS | tracking; tape jitter; tracking bands; dropouts; chroma bleed; time-base wobble | y-dependent scan offset; band center/height/speed; lower-res chroma, seeded glitches | moving band, line jitter, color smear distinct |
| FX107 CRT | curvature; scanlines; RGB phosphor; scan drift; persistence; signal interference | aspect-correct curved sampling; output-pixel phosphor mask; decay in seconds | mask period, decay vs time, nonzero curvature |
| FX108 glitch | glitch blocks; compression-style blocks; digital tearing; fragment split | held seeded block masks, quantized offsets and palettes | temporal hold and no-op boundaries |
| FX109 pixel | modular pixel dissolve; pixel sort; voxel dots; mosaic | dissolve threshold keyed per cell; sort along bounded rows/segments by luma/hue | dissolve endpoint; sorted monotonic segment |
| FX110 smear | horizontal; directional; radial; slit scan | map selected strip, accumulate samples or rolling history by axis | fixed ramp expected mapping |
| FX111 geometry | ripple; wave; kaleidoscope; polar; mirror/tile; perspective; magnifier; jigsaw | inverse UV mapping, aspect correct, edge modes | known coordinate fixture maps correctly |
| FX112 glass | reeded; fluted; microprism; frost; clear refraction | repeated height-normal profiles; roughness sampling | each profile has distinct direction/refraction |
| FX113 materials | chrome; liquid metal; emboss; holographic iridescence; specular | relief normals, environment lookup, Fresnel, animated bands | light/view shift changes highlights |
| FX114 texture | paper; craft paper; stucco; textile; glitter; wood; marble | separate original procedural noise/pattern recipes and palettes | visibly different distributions at fixed seed |
| FX115 print | CMYK print; risograph; ink bleed; registration error; comics | channel separations, misregistration, paper/ink compositing | channel offset and dot-angle fixtures |
| FX116 technical | CT scan style; thermal; wireframe; blueprint; contour; cutaway style | luma bands/scan sweep; geometry wireframe from actual triangles; region masks | clearly distinguish 2D styling vs 3D geometry |
| FX117 text/graphic | ASCII; code rain; topographic lines; annotations; shapes mosaic | rasterize bundled licensed glyph atlas; luma→glyph index; seeded column streams | exact glyph mapping and movement |
| FX118 temporal | frame delay; frame echo; afterimage; trails; accumulation; freeze/hold | seconds converted to canonical history frames; decay exp(-dt/tau); reset/clear | impulse decay; seek replay equals sequential |
| FX119 feedback | transform feedback; displacement feedback; reaction feedback | explicit prior-frame read/write, transform/mix/decay, bounded buffers | cycle accepted only through temporal edge |
| FX120 keys/mattes | luma key; chroma key; color range; despill; alpha extract | threshold/softness, color distance, morphology, RGB despill | known green foreground preserves edge alpha |
| FX121 masks | shape; path; feather; dilate/erode; invert; combine | R8/R16F mask math; feather radius px | mask ranges; union/intersection/subtraction |
| FX122 composite | source over; add; screen; multiply; overlay; soft/hard light; difference; exclusion; dodge/burn | explicit premultiplied blend formulas and eps clamps | CPU pixel oracle for alpha/colors |
| FX123 transforms | affine; crop; corner pin; camera shake; stabilize translation | inverse homogeneous transform, anchor, bounds; stabilization uses tracked data | marker coordinates; no cumulative drift |

Preserve every visual family from the old plan as a catalog entry. Actual depth inference, internal anatomy, physically correct metal, or unseen reconstructed geometry must not be implied by a visual-style preset. The UI help explains stylization when relevant.

## 4. Preset recipes

Ship at least these editable templates with parameter macros and generated thumbnails:

- Bass Impact: Blob → Chromatic metal → Bloom; bass scales deformation, onset triggers pulse, treble raises grain.
- VHS Tracking: source → chroma bleed → tracking → tape jitter → static → CRT scanlines, low default intensity.
- CRT Phosphor: source → curvature → RGB mask → scanline → persistence, output-pixel-aware phosphor.
- Editorial Halftone: grayscale color adjustment → halftone → paper grain, ink/paper controls.
- CMYK Print: CMYK screen channels → registration offsets → paper composite.
- Risograph: duotone layers → ink grain → misregistration → paper.
- Sierra Dither: dither mode Sierra, two-color palette, scale=1.
- Modular Pixel Dissolve: block mask → threshold audio/phase → dissolve with defined source-out endpoint.
- Industrial CT: luma segmentation → contours → scan sweep → monochrome technical overlay.
- Thermal Vision: gradient map thermal palette → soft bloom, no false temperature measurement.
- CAD Wireframe: true imported/procedural geometry depth/normal/wireframe → color composite; texture-only input uses separately labeled edge styling.
- Technical Cutaway: region mask combines two actual sources, optional 3D clipping plane.
- Reeded/Fluted/Microprism Glass: each real profile preset on refraction.
- Liquid/Chromatic Metal: relief/environment recipe plus slow phase motion.
- Water Caustics: animated generator → overlay/add onto source.
- Fractal Noise; Clouds; Nebula; Mesh Gradient; Moiré; Glowing Wave; Pattern Grid; Concentric Patterns: generator templates with animate toggle and period.
- Paper Texture; Blueprint; ASCII Art; Topographic Contours; Shapes Mosaic; Code Rain: corresponding kernels and text atlas.
- Cyberpunk Glitch: held slice shift + RGB split + block glitch; onset trigger, restrained default flash.
- Bloom and Halo: two independently adjustable glow layers, alpha policy visible.
- Flow Field Particles: curl field → particles → trails → bloom.

## 5. Catalog acceptance gate

The catalog linter checks IDs unique, schema valid, defaults within ranges, ports resolvable, resource policy specified, actual shader/kernel/template exists, help present, and fixture declared. It must reject unresolved nested preset references.

For every FX001–FX028 and every required family child: render default, neutral/zero, changed parameter, transparent edge, portrait aspect, half preview scale, and same-time repeat. Test specific distinguishing invariant from its row. Goldens alone do not prove an effect is implemented: at least one analytic/metamorphic assertion is required. Generate visual contact sheets and open representative outputs for inspection. Record precision/performance deviations, never silently change a reference image to hide a regression.
