# Concrete data, API, and resource contracts

Implement these contracts through generated schemas/types. Examples describe the wire representation; they are not substitutes for validated runtime models. IDs in examples are illustrative and must be generated uniquely in a real project.

## 1. Core document types

Canonical project time is a signed rational. Project clips start at>=0; source PTS may be negative before stream normalization. Normalize source presentation start explicitly using stream start time/edit list, retaining original PTS in asset metadata. Limit project duration to7 days initially and reject overflow rather than wrapping.

```typescript
type Rational = { numerator: string; denominator: number };
type Rate = { num: number; den: number };
type Range = { start: Rational; duration: Rational };
type ParamValue =
  | { kind: 'float'; value: number }
  | { kind: 'int'; value: number }
  | { kind: 'bool'; value: boolean }
  | { kind: 'enum'; value: string }
  | { kind: 'color'; value: [number,number,number,number] }
  | { kind: 'vec2'; value: [number,number] }
  | { kind: 'vec3'; value: [number,number,number] }
  | { kind: 'asset'; assetId: string }
  | { kind: 'path'; pathSetId: string };
```

Rust maps these to tagged enums; validate float finiteness and ranges. Native integer parameter values limited to signed32-bit unless a declared specialized type is added. Colors in saved parameters use documented linear RGB+straight alpha; runtime premultiplies when creating textures. UI converts its picker display-sRGB to saved linear. Enum strings are stable IDs, labels can change.

```json
{
  "schemaVersion": 2,
  "projectId": "550e8400-e29b-41d4-a716-446655440000",
  "name": "Bass Blob",
  "settings": {
    "size": {"width":1920,"height":1080},
    "frameRate": {"num":60,"den":1},
    "pixelAspect": {"num":1,"den":1},
    "audio": {"sampleRate":48000,"channels":2},
    "workingColor": "linear_rec709",
    "seed": "47019",
    "simulationStep": {"numerator":"1","denominator":120}
  },
  "assets": {},
  "timeline": {
    "range": {
      "start":{"numerator":"0","denominator":1},
      "duration":{"numerator":"8","denominator":1}
    },
    "trackOrder": [],
    "tracks": {},
    "clips": {},
    "transitions": {},
    "masterGraphId": null
  },
  "graphs": {},
  "mappings": {},
  "scenes": {},
  "takes": {},
  "markers": [],
  "extensions": {}
}
```

`assets`, `tracks`, `clips`, `graphs`, `mappings`, and `scenes` are ID-keyed maps with deterministic serialized key order. Explicit arrays define order where meaningful. Never depend on unordered hash-map iteration. Clip source tagged union: `asset`, `generatorScene`, `liveSource`, `freezeFrame`, `recordedTake`. Each declares version and required reference IDs. Clip graph and track graph optional; no graph means pass-through source.

Track schema: ID/name/kind, clip order, lock/mute/solo/visible, gainDb/pan/opacity/blendMode, graphId, automation lanes. Clip schema: ID/name/source, range, sourceRange, rate, loopPolicy, graphId, transform, crop, opacity, audio controls, automation, linkedGroupId. Transform stores position in normalized composition coordinates, scale ratio, rotation radians, anchor normalized source coordinates; UI pixels convert at boundaries. Source field units are explicit; no mixed pixel/UV coordinates in one parameter.

Graph instance identity includes scope+clip/track/master ID+graph ID; using the same graph preset on two clips creates independent temporal runtime state. Shared generator state requires an explicit shared source node/reference, not accidental preset pointer reuse. On scene duplication new IDs preserve asset references but reseed only when user chooses randomize.

## 2. Public command/event API

All frontend actions call a single typed request dispatcher or generated typed commands. Avoid ad hoc JSON event names scattered across components.

Request envelope:

```json
{
  "protocolVersion":1,
  "requestId":"uuid",
  "projectId":"uuid-or-null",
  "expectedRevision":"42",
  "command":{"type":"setParameter","nodeId":"uuid","parameterId":"radius","value":{"kind":"float","value":0.7}}
}
```

Response is success `{requestId,revision,result}` or error `{requestId,error:{code,message,detail,recovery,entityIds}}`. Job-producing operations return `{jobId}` immediately; progress/cancellation use job API. No blocking decode/network/export in request handler.

Commands required:

| Group | Commands |
|---|---|
| Project | create/open/save/saveAs/close/recover/collect/relink/validate |
| Assets | probe/import/removeReference/createProxy/cancelProxy/selectStream |
| Timeline | addTrack/addClip/move/trim/split/rippleDelete/insert/overwrite/slip/roll/retime/reverse/freeze/addTransition/setTrackState |
| Graph | addNode/removeNodes/connect/disconnect/setParameter/group/ungroup/exposeMacro/setBypass/savePreset/insertPreset |
| Automation | addKeyframe/moveKeyframe/removeKeyframe/setInterpolation/addMapping/setMapping/removeMapping |
| Transport | play/pause/stop/seek/step/setLoop/setRange/setMasterClock |
| Sources | enumerate/queryModes/open/close/reconnect/setCaptureRegion |
| Outputs | enumerateMonitors/openWindow/setFullscreen/configureSink/closeSink |
| Audio | enumerateDevices/selectSource/setMonitoring/setBus/setCalibration |
| Control | configureOsc/startLearn/stopLearn/configureMidi/setFeedback/selectBridge |
| Jobs | startExport/startRecording/stopRecording/cancelJob/getJobState |
| Diagnostics | getCapabilities/getCounters/exportBundle/clearError |

Events: DocumentChanged(revision,changed IDs), TransportSnapshot, FeatureSnapshot, SourceStatus, OutputStatus, JobProgress, Diagnostic, PreviewFrameReady. Every event includes project/session ID and monotonically increasing event sequence. UI discards old session events after project change. On an event-sequence gap reload relevant snapshot. Low-frequency snapshots do not contain frame bytes; preview frame channel is independent.

Event sequences and long counters serialize as strings. Bridge OSC32-bit sequence wraps modulo2^31; detect newer packets with modular arithmetic inside the active session and reset history on new hello/epoch. Do not compare signed sequence values naively across wrap.

## 3. Render data ABI

FrameUniform is exactly96 bytes with16-byte aligned blocks:

| Offset | WGSL field | Components |
|---|---|---|
| 0 | dims:vec4<u32> | output width,height,source width,height |
| 16 | sampling:vec4<f32> | inverse width,height,preview scale x,y |
| 32 | clock:vec4<f32> | time low seconds,time high seconds,delta seconds,simulation interpolation |
| 48 | counters:vec4<u32> | frame low32,frame high32,seed low32,transport epoch low32 |
| 64 | music:vec4<f32> | beat phase,bar phase,tempo,beat confidence |
| 80 | controls:vec4<f32> | wet,quality scale,simulation dt,reserved0 |

Time split: high=floor(seconds/1024)*1024, low=seconds-high. For periodic phase use reduced rational phase on CPU when known; summing high+low in f32 defeats precision protection. Frame-based hashes use integer counter blocks. Rust `repr(C)` layout tested against size/alignment/offset and harness encoder.

NodeParameterUniform is a manifest-generated list of16-byte slots with field metadata. Floats/colors/vec values use f32 vec4 slots; booleans/enums/integers use u32 vec4 slots; no implicit WGSL bool in uniform memory. Parameter order is schema order/versioned, not alphabetic reorder. Dynamic offsets align to device.min_uniform_buffer_offset_alignment. Distinguish payload96 bytes from allocation stride256 or queried device value.

AudioFeature storage records fixed scalar slots, spectrum/waveform lengths, timestamps and validity. Variable arrays include explicit count and capacity; shader clamps indexes to count. Use immutable per-frame snapshot so nodes see one feature revision per evaluation. AudioTexture upload rate follows analysis, not every effect pass.

Compute buffers declare capacity, live count, stride, byte size, binding read/write and reset value. All byte-size multiplications checked on CPU, dispatch count covers ceil(count/workgroup_size), kernel guards `index>=count`. Use workgroup size64 default where supported and explicit tested alternative if needed. No global cross-workgroup synchronization assumed within one dispatch. Multi-stage solvers use separate dispatches/buffers.

## 4. Runtime interfaces and ownership

Conceptual Rust interfaces below must become real types with cancellation/error behavior. Select idiomatic async/thread APIs consistent with pinned libraries; do not copy these signatures blindly when an API has required lifetimes.

- Source: `capabilities`, `open(config)`, `request_time(time,epoch)`, `poll_frame`, `poll_status`, `close`.
- Frame lease: asset/source ID, frame PTS/duration/time-base, epoch, color/orientation, CPU planes or owned GPU texture lease, release callback/RAII ownership. Never borrow decoder memory after next decode without retaining its frame reference.
- Node runtime: `prepare(device,manifest)`, `evaluate(frame_context,inputs)`, `reset`, `checkpoint`, `restore`, `dispose`.
- Export job: immutable snapshot, cancellation token, progress counters, encoder state, temporary paths, finalization transaction.
- Sync provider: tempo/beat/phase/playing/epoch/confidence/clock mapping; event arrival time separate from effective musical time.
- Native shim: versioned C ABI, opaque handle, create/destroy, poll/copy/send operations, explicit caller-owned buffers, error code + bounded error string.

C ABI rules: integers fixed-width, enum values fixed, no Rust Vec/String/C++ exception across boundary, no callbacks into freed state, no unwind across ABI. Functions returning pointers declare who frees and on what thread. Frames acquired from NDI/Syphon/etc release exactly once. Copy route copies before release; zero-copy lease retains until GPU synchronization completes. Test repeated create/open/close, failed partial initialization and cancellation.

## 5. Catalog manifest shape

```json
{
  "id":"fx.pixelate",
  "version":1,
  "requirementIds":["FX017"],
  "name":"Pixelate",
  "category":"Print and Pixel",
  "implementation":{"kind":"fragment","path":"pixelate.wgsl","entry":"fs_main"},
  "inputs":[{"id":"source","type":"Texture2D","required":true}],
  "outputs":[{"id":"color","type":"Texture2D"}],
  "parameters":[
    {"id":"blockSize","kind":"vec2","default":[8,8],"min":[1,1],"max":[512,512],"unit":"output_px","automatable":true,"modulatable":true,"stateImpact":"none"}
  ],
  "timeModel":"analytic",
  "colorPolicy":"linear_premultiplied",
  "qualityClass":"L",
  "fixtures":["pixelate_blocks","pixelate_alpha"],
  "help":"Groups samples into editable pixel blocks."
}
```

Shader IDs may use snake_case internally while saved parameter IDs are stable camelCase. Consistency is enforced by registry generation. Built-in common parameters wet/bypass/seed are injected by manifest compiler and documented; do not manually duplicate their definitions in every manifest.

All catalog descriptors additionally give soft ranges, scale, smoothing default, external native/normalized conversion, edit reset/recompile impact and sample mappings. If family-specific defaults are not written elsewhere use these exact defaults: enabled=true; intensity/amount/opacity=1 where identity is not the preset goal; strength in pixels=12; radius=8px; speed=.2cycles/s; angle=0rad; threshold=.5; softness=.05; count=16; seed inherited; palette black→white; border clamp; resource dimensions inherited unless simulation256²; state resets only for resource/topology changes. Override explicitly when those values are inappropriate for the actual named algorithm and record rationale with tests. Never choose a random default at run time.

## 6. Output/capability/evidence records

Capability fields: component ID, platform, available bool, implemented bool, verified bool, version, mode list, limitations, missing prerequisite, evidence. Do not derive `implemented` from device presence or `verified` from `available`. UI uses available status; final report uses all fields.

Output config: ID, sink type, name, resolution, rate, fit policy, color transform, pixel format, alpha policy, audio bus, latency, status. Rate conversion repeats/skips presentation frames without changing simulation cadence. Different monitor refresh rates never create new project time samples.

Test record fields: test ID, specification version, source revision and dirty hash, timestamp, OS/arch/GPU/backend/driver, dependency lock hash, fixture hash, settings, command and exit, status, measured values, expected bounds, artifact hashes, failure detail. Status enum passed/failed/unavailable/not_run. No human-readable success string overrides a failed exit or unavailable prerequisite.

Requirement record fields: ID, title, class required/platform_required/conditional/extension, task IDs, acceptance IDs, implementation paths, evidence paths, status, blocker. Catalog family variants generate child records. Status verified requires actual passing evidence for that requirement, not just a broad task record.
