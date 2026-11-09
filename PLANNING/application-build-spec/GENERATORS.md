# Generators, animators, scenes, and procedural inputs

Generators are first-class sources that can enter any effect graph and timeline. Animators consume an existing asset, image, path, or scene and output an animated result. Simulation nodes evolve state over time. The source picker offers Files, Live Inputs, Generators, Animated Assets, 3D Scenes, and Recorded Takes.

## 1. Shared contract and catalog coverage

Every row ID below belongs in requirements.json. Rows may declare an explicit variant enum; every listed variant must work. An existing FX fill node can satisfy a generator row through a shared implementation, but the catalog must expose the generator route and all controls. No duplicated visual algorithm is required.

Generator manifest extends the effect manifest with `generator_id`, version, category, ports, parameters, `time_model` (analytic/fixed_step/canonical_frame/external), `seek_policy`, `loop_policy`, determinism scope, checkpoint schema, GPU requirements, quality controls, resource estimate, optional dependency, and sample project. Every rendered source outputs Texture2D with alpha and metadata; underlying PathSet, SdfField, Geometry, ParticleBuffer, VectorField, Depth, Normal, and Event outputs remain connectable through typed nodes.

Common controls: resolution inherit/custom; seed; duration default 8 s; start time; speed; phase; loop period default 4 s; enabled; background transparent/solid; transform; opacity. Analytic periodic generators compute phase from `(local_time/period) mod 1`. Not every animation is naturally seamless: show loop status exact/approximate/restart. Exact loop mode uses periodic noise/path construction, baked cycle, or verified matching end state; never label a reset pop seamless.

Defaults and maxima in this file are initial product limits, adjustable only through validated capability settings. A custom-source editor offers an explicit Advanced route. Ordinary generator users select presets, edit controls, and map audio with the same inspector as effects.

## 2. Procedural 2D and mathematical generators

Implement a shared WGSL field library. Noise octaves max 8, fractal iterations max 512 default 128, sampling budgets explicit. Use periodic-coordinate noise for seamless loops. All outputs are deterministic at a given time/seed within selected backend numerical tolerance.

| ID | Generator and variants | Native controls/defaults and construction |
|---|---|---|
| G001 | Solid/color plate | RGBA white; premultiplied color; transparent color plate supported |
| G002 | Linear/radial/conic gradient | 2–32 stops, origin/angle/radius, repeat/clamp; analytic coordinate mapping |
| G003 | Mesh gradient | FX024 contract, animated control grid |
| G004 | Noise/fBm/turbulence/ridged | FX022 plus abs/ridge transform, scale=4, octaves=5 |
| G005 | Voronoi/cellular/crackle | density=12 cells/axis, jitter=1, nearest/second/difference distance modes, speed=0.1 |
| G006 | Plasma | bounded sum of sinusoids + palette, frequency=4, speed=0.2 |
| G007 | Grid/dots/checker/triangular/hex | FX027; output mask and cell center instance positions |
| G008 | Concentric/radial rays/spiral | FX021; rays=32, spiral turns=8, center, speed, stroke width |
| G009 | Moiré/interference | FX025; stripe/ring/grid combinations |
| G010 | Halftone/hatching pattern fill | independent pattern driven by gradient/noise/constant tone=0.5 |
| G011 | Topographic contours | field→iso-lines, levels=12, width=1.5 px, animated terrain/noise |
| G012 | Clouds/nebula/aurora | FX020/FX026 plus aurora layered ribbons in warped coordinate space |
| G013 | Glowing wave/oscilloscope | FX023, optional AudioSpectrum/waveform, transparent background |
| G014 | Water caustics/ripples | FX028; ripple impacts explicit events and radiating wave distance |
| G015 | Mandelbrot/Julia/Burning Ship | center, zoom=1 [0.01,1e6 baseline f32], Julia c, iterations, palette; show precision ceiling |
| G016 | Lissajous/rose/spirograph | ratios=3:2, radii, phase, line width, trail length; PathSet + image |
| G017 | Superformula/superellipse | bounded m,n1,n2,n3, radius and polygon sampling=256; singularity guards |
| G018 | Truchet tiles/maze | seeded tile orientation; tile=48 px, stroke=3 px; continuous tile borders |
| G019 | Texture fields | wood/marble/paper/fabric/stucco/glitter recipes from distinct noise/pattern combinations |
| G020 | Starfield/galaxy/space tunnel | points=5000, seed, speed, depth, parallax, palette; finite near clipping |
| G021 | Scan/test signal | bars, ramps, frame counter, checker, moving marker, SMPTE-style original test layout; for diagnostics |
| G022 | Polar tunnel/vortex | palette/texture mapped to atan2/radius, twist/speed; bounded singular center |
| G023 | Fractal tree/L-system | grammar asset, angle=25 deg, depth=5 max10, segment limit 100k; bounded expansion |
| G024 | Strange attractor plot | Lorenz/Rössler/Clifford, dt bounded, burn-in, points=20k; stateful fixed-step trails |
| G025 | Kaleidoscopic pattern | primitive/noise field, symmetry=6 [2,64], mirror, spin, radial scale |
| G026 | Recursive geometry/tiling | Sierpinski triangle/carpet, Koch curve, valid Penrose rhombus substitution with matching rules; default depth4 max8, bounded tile count and view clipping |
| G027 | Cellular automata | life, elementary rule=30, cyclic color automaton; grid=256², steps/beat, wrap, initial seed/image |
| G028 | Reaction-diffusion | Gray–Scott U/V, feed=.0367, kill=.0649, Du=1,Dv=.5, grid256², fixed stable substeps |

Test field coordinate transforms with known ramps, noise periodic seams, fractal changing parameter, grammar limits, automaton blinker, and reaction stability/mass bounds. An unexpected numerical overflow stops that node with a visible error and preserves last valid frame.

## 3. SVG and vector animation

Parse a safe supported SVG subset. Preserve groups, paths, fills/strokes, transforms, clipping/masks, viewBox, IDs, and text converted with explicit font resolution. Disallow scripts/external resources/foreignObject. Present unsupported filters or CSS animation in import report. Imported SVG is not automatically a full browser document. Read supported shape trees into an editable internal VectorScene. Rasterize using ThorVG; initial CPU-render/upload path is acceptable, cache static layers. ThorVG WebGPU/native texture interop is an optimization requiring same-device compatibility tests.

The application animation model owns time and properties. On each requested frame evaluate transforms/colors/paths at exact local time then render. Do not rely on requestAnimationFrame/CSS clocks for export.

| ID | Animator/source | Required behavior |
|---|---|---|
| G030 | SVG source | viewBox fit/fill/stretch, selected group, alpha, layer tree, reimport/relink |
| G031 | Primitive shape/path editor | rectangle/ellipse/polygon/star/Bezier, fill/stroke/dashes, handles, anchor/transform |
| G032 | Trim paths/draw-on | start/end 0..1, offset, length-correct sampling, sequential/simultaneous subpaths |
| G033 | Follow path | asset/shape follows PathSet by arc length, tangent orientation, loop and easing |
| G034 | Path morph | match compatible topology or resample contours to common points; reverse/winding/starting point match; expose unsupported multi-contour pairing |
| G035 | Shape morph | circle/square/star/custom sampled outlines, topology-aware interpolation, no random point shuffle |
| G036 | Repeater | line/radial/grid/path placement, count=16 max10k, stagger, per-instance transform/color |
| G037 | Vector deform | sine/noise/bend/twist, control point/sampled-outline deformation, bounds clipping optional |
| G038 | Stroke effects | animated width/dashes, traveling highlight, variable-width approximated tessellation where needed |
| G039 | Gradient/fill animator | animate stops, hue and coordinates, preserve gradients rather than flatten at import |
| G040 | Vector mask/clip animation | animated paths export as Mask and used by arbitrary images/generators |
| G041 | Vector to particles | sample along outline/interior, spacing/seed/density, positions/normals/color |
| G042 | Vector to mesh | flatten curves with tolerance=0.5 px, triangulate holes, extrusion=.1 m/bevel optional; Geometry output |
| G043 | Lottie source | actual JSON/.lottie asset support through reviewed parser/runtime; frame seek, loop, speed, markers, color overrides when supported |
| G044 | Animated icon/logo scene | grouped SVG/shape layers with stagger, elastic/easing templates; original bundled samples |
| G045 | Hand-drawn vector loop | deterministic wobble/jitter, fps hold=12, amplitude=1.5 px, periodic seeded sequence |

SVG morph defaults 256 samples/contour max4096; error tolerance adjustable. Hole winding is preserved. Cache flattening unless path geometry changes. Lottie feature support is measured using bundled fixtures; unsupported expressions/effects produce import warnings. Raw Lottie playback is independent of any After Effects installation. Arbitrary SVG animation may be imported as a recorded image sequence when safe native conversion cannot reproduce it.

## 4. Image, collage, and video animation

These animators accept Texture2D, so generated images, videos, cameras, or previous effect outputs work equally.

| ID | Animator | Required controls and mechanism |
|---|---|---|
| G050 | Transform/Ken Burns | position/scale/rotation/skew/anchor; start/end framing and ease, auto-fit bounds |
| G051 | Perspective/corner pin | four corners, inverse homography validation, reject singular transforms |
| G052 | Mesh warp/puppet | grid default8×8, pin handles, weighted deformation; keyframeable pin positions |
| G053 | Displace/liquify | external field/noise, radius/strength, recorded brush strokes feed deterministic field |
| G054 | 2.5D layered parallax | cutout layers + optional depth texture, virtual camera, depth scale; reveal holes via manual background or explicit fill approximation |
| G055 | Heightfield image | luminance/depth→mesh elevation, resolution128², camera/lights; no claim of unseen reconstruction |
| G056 | Image to particles | sample RGB/alpha/luma into point colors/positions, scatter/attract/morph, count20k |
| G057 | Image fragmentation | grid/Voronoi fragments, pivot/velocity/rotation, physics optional, same source UVs per shard |
| G058 | Image slices/accordion | band count32, per-band delay/rotation/offset; no unbounded geometry allocation |
| G059 | Slit scan/time displacement | select historical frame by x/y/aux map, history2s default capped by memory; seek/replay rules |
| G060 | Polar/sphere/cylinder projection | UV transform or actual mesh mapping with seam policy and camera |
| G061 | Collage/cutout | multiple assets, masks, procedural layout, weighted z-order/stagger, transparent layers |
| G062 | Sprite/flipbook | sheet rows/columns, frame order/rate/hold/loop/pingpong; image sequence/GIF/APNG decode |
| G063 | Motion brush/cinemagraph | user-painted region and direction, periodic advection/wave; stationary outside, feather and hole policy |
| G064 | Optical-flow advection | precompute/stream dense flow, confidence/occlusion, move particles or warp frames; quality/cost explicit |
| G065 | Image morph/dissolve | crossfade plus mesh/flow-assisted morph only with valid correspondence; unsupported correspondence does not masquerade as morph |
| G066 | Image field extraction | luma/alpha/edge/color→Mask/VectorField/height/point positions; thresholded mask→signed 2D distance field using exact CPU distance transform or validated GPU jump-flood approximation; declare pixels as distance units |
| G067 | Infinite zoom/Droste | nested transformed image with bounded repetitions, center/zoom period/phase |
| G068 | Stop-motion animator | quantized-time hold, jitter, exposure variation, deterministic seed; base image/video source |

G064 is required without optional learned models: implement a deterministic image-pyramid block-matching or reviewed non-neural flow baseline at bounded analysis resolution, produce confidence/occlusion masks, and feed advection. G170 is the optional upgraded OpenCV/model analysis adapter. Never make the required image animator depend solely on unavailable optional inference.

Image motion templates: breathing poster, floating collage, waving flag image, photo orbit, shattered logo, flowing album artwork, pulsing bass cover, pixel flock portrait, rotating paper cutouts. Every template exposes editable nodes and maps to actual controls.

Depth input can be user-provided, manually painted, or generated by an optional model. A single image with a depth map cannot reveal real hidden surfaces; inspector exposes fill/backplate options. Imported alpha edges remain clean in deformation and particles.

## 5. Typography, diagrams, and information graphics

Use actual text shaping, Unicode/font fallback, line breaks and alignment. Cache glyph atlas or vector glyph outlines; supply at least one bundled distributable font and its license. Dynamic text updates invalidate layout safely. Use glyph IDs, not byte positions, for per-character animation. Support ligatures and RTL through shaping; if complex-script per-character split is ambiguous animate shaped clusters.

| ID | Generator | Required behavior |
|---|---|---|
| G070 | Text plate | content, font asset, size/alignment/tracking/line height, fill/stroke/shadow |
| G071 | Kinetic type | by glyph/word/line: position/rotation/scale/opacity/wave/stagger, intro/hold/outro |
| G072 | Typewriter/scramble | reveal shaped clusters, cursor, deterministic random substitution, speed/events |
| G073 | Text on path/ring | PathSet, arc-length placement, tangent rotation, spacing, scrolling |
| G074 | 3D text | triangulated/extruded glyph outlines, holes, bevel, material, scene lighting |
| G075 | Text to particles | outline/interior sampled emission, attract-to-text and release events |
| G076 | Physics type | glyph/word rigid bodies with colliders, gravity/bounce/springs; layouts remembered |
| G077 | Counters/clock/data labels | numeric format, selected clock/data stream, deterministic offline timestamp or recorded take |
| G078 | Charts/radar/dashboard | CSV/JSON values→bars/lines/radial chart/radar; bounds, labels, timeline interpolation |
| G079 | Technical overlays | grids, rulers, dimensions, object labels, scan cursors, callouts, original HUD templates |

Text is usable as image, Mask, PathSet and Geometry where appropriate. Do not generate all text into browser DOM screenshots as the only native export solution.

## 6. 3D SDF blobs and procedural fields

Implement a bounded SDF expression tree and shader-generation/compiler path in Rust. It is a typed domain structure, not unrestricted code. Nodes specify primitives and transforms; max128 SDF operations/default32; max loop/repetition bounds validated. Compile/cache when structure changes; numeric parameters remain uniforms. Use analytic derivatives when available or epsilon-scaled central differences. Clip scene to declared bounds and use near/far plane.

| ID | Generator/operator | Required variants/controls |
|---|---|---|
| G080 | SDF primitives | sphere, box, rounded box, capsule, cylinder, cone, torus, ellipsoid, plane, hex prism |
| G081 | Blob/metaballs | count4 max64, radii=.5 m, smooth merge=.3 m, orbit/noise/beat deformation |
| G082 | SDF Boolean | union, subtract, intersect, smooth versions, blend radius; consistent signed distance approximation |
| G083 | SDF domain | translate/rotate/scale, repeat, mirror, twist, bend, taper, elongate, shell/onion |
| G084 | SDF noise displacement | amplitude=.1 m, frequency2, octaves3, time speed; conservative step safety |
| G085 | Gyroid/lattice | threshold/thickness/scale, bounding volume, animated phase; fractal distance estimator separate |
| G086 | Fractal bulb/box | bounded iterations12 default max32, bailout, power/scale; quality H with hit safeguards |
| G087 | SDF render | color/Depth/Normal/Mask, steps96 [16,256], epsilon relative to pixel footprint, max_distance100 m |
| G088 | SDF material | diffuse, chrome, liquid metal, iridescent, emissive, translucent approximation, normal/debug |
| G089 | SDF mesh conversion | marching cubes within finite bounds, resolution64 default max256, async cancellable job, Geometry + GLB export |

Default blob scene: 4 spheres with smooth union, slow orbital motion, cool metallic material, one key/one fill light, transparent background, bass input raises radius and displacement, onset temporarily increases emission. Expose camera distance and mesh/raymarch mode.

Sphere tracing uses bounded steps and no-hit transparent output; report iteration stats. Displacement shrinks marching step to prevent overstepping. Raymarch render optional soft shadow/AO with quality controls. Refractive material samples scene/background through an explicitly approximate screen-space/environment method; physically traced multi-bounce transmission is outside baseline. Raymarched depth writes into a comparable linear depth texture so SDF/mesh scenes can composite correctly.

3D SDF kernels must have portable WGSL tests with analytic sphere depth at center, normal direction, Boolean operations, max-step termination, negative/zero scale guards, and correct bounds.

## 7. Mesh scenes, camera, material, and procedural geometry

Build a small scene layer in Rust over wgpu. It needs transforms, cameras, lights, PBR material, geometry instances and animation; a full game engine is unnecessary. Coordinate convention: right-handed, +Y up, meters, camera looks -Z, texture UV y mapping explicitly converted. Quaternions normalized. glTF coordinate conversion/import tested once, not hidden in every shader.

| ID | Scene node | Required behavior |
|---|---|---|
| G090 | Mesh primitives | box, sphere/icosphere, plane, disk, cylinder, cone, torus, capsule with segment limits |
| G091 | Parametric surfaces | torus knot, helix/tube, Möbius strip, superquadric; sampled UV mesh with normals |
| G092 | Path extrusion/ribbon | tube along path with stable parallel-transport frames, taper/twist, ribbon width |
| G093 | Mesh deformer | bend, twist, taper, wave, noise, lattice, audio displacement; normals updated |
| G094 | Geometry instancer | grid/radial/path/surface/image-driven placements, transforms/colors, max1M checked |
| G095 | Terrain/heightfield | noise/image height, grid128², color by height/slope, displacement amplitude |
| G096 | Voxel scene | occupancy from noise/image/SDF, cubes or surface mesh, grid64³ default with memory cap |
| G097 | Point cloud | PLY positions/colors import, point size, depth test, filtering, downsampling, orbit |
| G098 | Model scene | GLB/glTF import, buffers/textures, PBR metallic/roughness, animations, skin/morph targets supported and tested |
| G099 | Scene composition | merge scene objects/lights/cameras, namespace IDs, render layers, depth/mask outputs |
| G100 | Camera | perspective/orthographic/isometric, near/far/FOV, orbit/dolly/path/look-at/follow, shake |
| G101 | Lights/environment | directional, point, spot, hemisphere/ambient, environment texture; bounded shadow maps |
| G102 | Material | base color/metal/rough/emission/normal/alpha, texture assignment; stylized toon/wireframe |
| G103 | Scene animation | keyframes, glTF clips, blend/crossfade, loop, hierarchy transforms, camera path |
| G104 | Procedural architecture | repeat building blocks, skyline/city grids, seeded heights/windows, finite tiles |
| G105 | Planet/asteroid/galaxy | displaced spheres, rings, orbit fields, instancing; nested scaled coordinate precision |
| G106 | Ocean surface | sum-of-sines/Gerstner waves baseline, foam approximation, sky reflection; FFT ocean optional quality variant |
| G107 | Volumetric field | bounded density grid/noise, raymarch smoke/cloud/fire appearance, slices/cost controls |

Default scene renderer: opaque depth pass, alpha blend sorted by depth/object ID, directional shadow optional, environment map optional, exposure/color output in shared working space. Alpha-masked versus blended material explicit. Model support matrix lists unsupported glTF extensions instead of dropping content. Imported assets must stay portable and relinkable. Terrain, city, and galaxies use bounded LOD/instance count, no endlessly growing world.

Camera shake deterministic noise, amplitude in meters/degrees, frequency Hz, trauma decay seconds, audio/OSC trigger input. Depth of field is FX002, motion blur FX103 uses actual motion vectors where available.

## 8. GPU particles, trails, and agent systems

Implement one particle kernel with fixed capacity, deterministic spawning, free/alive index buffers or stable compaction, double-buffered state and sorted event handling. Per particle: stable ID, position/previous, velocity, age/lifetime, size, rotation, color, seed, custom data. Counter updates never index past capacity. CPU control changes are bounded uniform/event uploads. Buffer readback is for checkpoints/diagnostics, not every live frame.

Default capacity20k, maximum1M subject to GPU budget, spawn1000/s, lifetime3s, fixed dt1/120. Spawn counts accumulate fractional emissions deterministically. Onsets/MIDI create event bursts with timestamps; duplicate network packet sequences do not double-emit.

| ID | Particle node | Required controls/behavior |
|---|---|---|
| G110 | Emitter | point, line, circle, rectangle, sphere/volume, mesh surface, path, image, text; rate/burst/seed |
| G111 | Initializer/lifetime | direction/cone/speed/range, lifetime distribution, initial color/size/rotation |
| G112 | Forces | gravity, wind, drag, curl noise, vector field, attract/repel, vortex/orbit, springs |
| G113 | Over-life curves | color/alpha/size/velocity/rotation curves normalized age, sampled LUTs |
| G114 | Collision | plane/sphere/box/depth/SDF, restitution/friction, kill/bounce/slide; field spaces explicit |
| G115 | Renderer | points/billboard sprites/instanced mesh, alpha/additive, soft-depth particles |
| G116 | Trails/ribbons | bounded points per particle default16 max128, age fade, stable strip orientation |
| G117 | Morph/attractor target | image/text/geometry target points, blend strength, stable correspondence by particle ID |
| G118 | Boids/flocking | separation/alignment/cohesion, bounded neighborhood spatial grid, max neighbors32, avoid O(N²) baseline |
| G119 | Agent trails/slime field | particles sample/turn/deposit into field, diffusion/decay compute, bounded buffers |
| G120 | Event output | birth/death/collision events, per-frame event cap1024, connect to control/generators |

Ship particle templates: dust, snow, rain, sparks, confetti, fireflies, star tunnel, galaxy, fireworks, energy ribbon, smoke sprites, image flock, typography dissolve, bass bursts, orbiting mesh swarm. Templates reuse kernels but visibly exercise different shapes/forces/renderers. No template requires paid assets.

GPU parallel compaction/atomic ordering may not be bit-identical across vendors. Require deterministic IDs/spawn schedules and bounded numeric/image tolerance. Export on the same selected backend/config must repeat. If strict simulation replay requires recorded checkpoints/takes, record that requirement; do not claim cross-GPU bitwise reproducibility.

## 9. Rigid bodies, elastic animation, and materials

Use Rapier for 2D/3D rigid bodies. Its step runs on simulation worker, independent of monitor refresh. Fix integration parameters, stable collider insertion order, and seed. Geometry rendering consumes immutable transforms; simulation cannot mutate the authoritative document.

| ID | Physics node | Required behavior |
|---|---|---|
| G130 | Rigid-body world | gravity=(0,-9.81,0), body limits, fixed dt, broadphase, reset/checkpoint |
| G131 | Body/collider | fixed/dynamic/kinematic, box/sphere/capsule/convex hull; mass/friction/restitution |
| G132 | Joint/chain | fixed, hinge, rope, spring; chain generator from anchor/path |
| G133 | Pendulum system | single/double/array, length/mass/damping, event impulses |
| G134 | Physics shards | precomputed Voronoi/grid fracture, convex pieces, image/text texture mapping |
| G135 | Cloth/flag | position-based grid32² default, structural/shear/bend constraints, pinned points, wind/collisions |
| G136 | Jelly/soft mesh | bounded spring/PBD mesh, volume-preservation approximation, stiffness/damping, collision planes |
| G137 | Elastic motion | critically damped springs for scalar/vector transforms, overshoot/bounce templates |

Cloth/soft bodies use their own tested PBD solver; do not claim Rapier rigid bodies provide full soft-body simulation. Constraint iterations default6 max32; prevent exploding geometry with bounded dt and parameter ranges. Node reports approximation/limits in help. Test equilibrium, free fall, bouncing energy bounds, pin stability, checkpoint/seek.

## 10. Fluids, waves, sand, and field simulations

Every simulation renders a texture and exposes its useful field(s). Parameters distinguish simulation resolution from output resolution. Reset from seed/image and pointer/OSC/audio impulses are recorded. A sampled external event is applied to the proper simulation tick.

| ID | Simulation | Required baseline and controls |
|---|---|---|
| G140 | 2D stable fluids | velocity/dye, advection, divergence, pressure Jacobi20 iterations, projection, vorticity; grid256² |
| G141 | Ink/smoke fluid styles | G140 parameter/material templates, viscosity, dissipation, dye palette, emitters |
| G142 | Shallow water/ripple tank | height/velocity grid256², stable wave dt/CFL constraint, damping, impulse radius/strength |
| G143 | Reaction-diffusion | shared G028, brush/image seeding, palette and evolving height outputs |
| G144 | Sand/falling cellular material | integer grid256², alternating deterministic update phases, sand/water/walls; texture/mask |
| G145 | Particle fluid | bounded SPH/PBD fluid default10k particles, neighbor spatial grid, timestep/iterations, surface sprite rendering |
| G146 | Magnetic/electric field | point charges/magnets, analytic vector field, streamlines, particle tracer; visualization rather than physical equipment control |
| G147 | Wave interference | multiple emitters frequency/phase/amplitude, distance attenuation, beat synchronization |
| G148 | Volume smoke/fire | stylized bounded 3D density-advection grid64³ default or procedural field; actual solver versus procedural mode labeled |

Baseline stable-fluids path: inject forces/dye→advect velocity→vorticity→divergence→pressure iterations→project→advect dye→decay. Boundary policy no-flow/wrap explicit. Require finite output, divergence reduction, dt stability, repeatable injected impulse, reset/checkpoint tests. 3D fluid surface reconstruction and high-resolution physically accurate liquid simulation are extension quality levels; do not advertise research-quality FLIP as completed based on a 2D warp.

## 11. Music, live data, and procedural control sources

These sources output values/fields/events and can become textures through rendering nodes.

| ID | Source | Required behavior |
|---|---|---|
| G150 | Audio spectrum/wave/spectrogram | selected source/bus, bin scale, smoothing, time span, colors; raw feature outputs too |
| G151 | Beat/rhythm generator | internal tempo, pattern steps=16, Euclidean pulses, probability seeded, subdivisions/swing |
| G152 | Oscillator bank | LFO shapes, phase offsets, additive waves, chaos only with bounded stable integration |
| G153 | Envelope/trigger | ADSR, envelope follower, retrigger/legato, event delay, sample-and-hold, random-on-beat |
| G154 | MIDI visual source | note grid, piano-roll trails, chord constellations, velocity-driven objects, MPE dimensions |
| G155 | OSC value/event source | learn, address/type, stale policy, smoothing, timestamped recorded take |
| G156 | CSV/JSON chart source | schema columns, time mapping, interpolation, rows/values→layout/geometry/particles |
| G157 | Network data source | WebSocket/HTTP polling, validated bounded schema, recording, offline replay |
| G158 | Interaction source | mouse/tablet pressure/keyboard/gamepad, normalized coordinate spaces, record/loop/replay |
| G159 | Clock/metadata source | project time/UTC anchor/filename/marker values, deterministic export substitutions |
| G160 | File watcher/asset sequencer | folder/image selection by beat/index/random seed, explicit permissions and stable list snapshot |
| G161 | Event sequencer | scene recall, control macros, finite timed cues, quantized next-beat/bar transitions |
| G162 | Control math | remap, clamp, smooth, lag, invert, quantize, gate, compare, sum/product, expression AST |

Scene switching default crossfade500ms, optional next beat/bar. Initialize next scene ahead of switch, compile pipelines in background, step separate state once active. Budget simultaneous scenes during crossfade; fallback is explicit hard cut if user permits, not frozen UI.

## 12. Optional computer vision and learned-data adapters

Vision inference is conditional on a specified model/runtime being present. Core image/geometry effects do not depend on it. Downloading weights needs an allowed source, checksum, license, size disclosure, and explicit enablement. Do not bill external APIs or use private credentials automatically.

| ID | Conditional source | Actual output/usage |
|---|---|---|
| G170 | Optical-flow analysis | frame-pair flow + confidence, OpenCV/native adapter, particles and image animation |
| G171 | Face/hand/pose landmarks | normalized positions/confidence/identity, timestamps, smooth/filter, attach emitters/cameras |
| G172 | Person/foreground segmentation | Mask with confidence/temporal smoothing, silhouette→particles/background replacement |
| G173 | Monocular depth | Depth with relative/metric declaration, camera parallax/mesh, confidence and holes |
| G174 | Object/marker tracking | tracked transform/points, attach text or scenes; lost/reacquired state |
| G175 | Webcam motion heatmap | frame difference/background model, motion field, region activity triggers |
| G176 | Gaussian splat scene | asset importer/runtime adapter, camera rendering to frame/depth where available, byte/count limits |
| G177 | Neural animation source | externally generated image/video/latent outputs become normal assets/takes; latency and frame availability explicit |

Use MediaPipe/OpenCV/ONNX Runtime only for the capabilities actually supported by selected build/model. Process inference at bounded resolution/rate, e.g.640 px/15–30Hz; render interpolates last valid data and shows latency. GPU inference must not starve render queue. No-model state preserves editable nodes with `MODEL_MISSING` and clear requirements; it is not verified feature completion. Local-only privacy settings disable library telemetry when supported or document actual runtime behavior.

## 13. External runtime and creative coding adapters

| ID | Conditional adapter | Contract |
|---|---|---|
| G180 | WGSL custom generator | constrained shader manifest, compiler diagnostics, bounded resources and execution watchdog strategy |
| G181 | p5/Canvas sketch | isolated browser worker/document with explicit renderFrame(time,inputs), deterministic virtual clock or recorded capture |
| G182 | Three.js scene | isolated renderer, frame texture/bytes adapter, camera/scene controls, export only with deterministic stepping |
| G183 | Lottie/Rive-like animation adapter | native Lottie core G043; other runtime formats only with actual dependency/parser and frame API |
| G184 | Python/process generator | explicit user-enabled executable/config, bounded RGBA frame/control pipe, timeout/crash recovery, no command execution from imported presets |
| G185 | External app feed | existing screen/Spout/Syphon/NDI route, frame/time metadata and recording; works with user-selected visual apps |

Custom WGSL is executable GPU work: validate size/loops where possible, scope resources, expose stop action; process isolation does not guarantee recovery from a driver hang. No arbitrary script runs simply because a project references it. External p5/Three examples are optional adapters, not a second native graph renderer. A plugin runtime that cannot seek is live-only and becomes exportable after capture.

## 14. Required generator templates and tests

Ship these scenes using original/generated assets: breathing 3D blob, smooth-merge metaballs, audio torus knot, iridescent gyroid, morphing SVG logo, path-drawn typography, pulsing mesh gradient, depth-image parallax with manual depth map, exploding image particles, flocking portrait, ink fluid, reaction-diffusion skin, bouncing text, cloth flag, star tunnel, procedural city, ocean plane, oscilloscope, MIDI constellation, OSC-controlled camera, data-driven radial chart, feedback kaleidoscope, cellular-automaton print, and a mixed scene→CRT→VHS composition.

For each required row test default frame, non-default control, empty input, transparency, time=0/time=1, seed repeat, reset, loop boundary, save/load, graph connection, and export/reimport. Stateful rows additionally compare sequential stepping with checkpoint/seek replay and different presentation rates (30 vs60 vs120) using identical simulation ticks. Visual equality has backend tolerance; event counts/IDs/timestamps must match exactly. Source scenes must remain editable after preset insertion.
