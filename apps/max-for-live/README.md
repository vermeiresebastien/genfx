# Flux Max for Live bridge (source devices)

This directory contains the **source** Max for Live devices and the OSC bridge
protocol shared with the desktop application. Building the `.amxd` binary requires
a Max for Live / Max 9 host; that packaging step is an external prerequisite. The
source patches, the OSC encoding contract, and the conformance test are provided
and verified here.

## Devices
- `flux-audio-bridge.maxpat` — audio analysis bridge (RMS/peak, bands, onset) over OSC.
- `flux-midi-bridge.maxpat` — MIDI pass-through + normalized control over OSC.

## Protocol (desktop side: `flux-integrations::bridge`)
The bridge speaks standard OSC 1.0 to `127.0.0.1` (configurable). Desktop binds a
UDP port and decodes:

```
/flux/v1/bridge/{instanceId}/hello       string protocol, int replyPort
/flux/v1/bridge/{instanceId}/features    int seq, double seconds, 8 floats
/flux/v1/bridge/{instanceId}/transport   int seq, double seconds, double beat, float tempo, int playing
/flux/v1/bridge/{instanceId}/heartbeat   int seq
```

Feature order: RMS, peak, bass, mid, treble, onsetStrength, onsetFlag, confidence.
`seconds` is bridge-relative (anchored at handshake); a transport discontinuity
starts a new epoch. The desktop replies with its session/instance identity.

## Audio path
`plugin~ → (unity pass-through) → plugout~`. Analysis taps the input with built-in
MSP filters/envelope following; snapshots are taken at 60 Hz and deferred to a
low-priority UDP queue. **No audio samples are sent over OSC** — only features.

## Packaging to `.amxd` (external step — needs a Max host)

> **The patches are already fully wired.** Each `.maxpat` already contains the
> `node.script bridge-osc.js @autostart 1` object, a `loadbang` to initialize it, and the
> connections (`plugin~ → snapshot~ → node.script → udpsend` for audio;
> `midiin → midiparse → node.script → udpsend` for MIDI). You do **not** need to add or
> connect anything — just open and save as a device.
>
> **Why the agent can't finish this:** `.amxd` is written only by Max's own save dialog.
> Max (Ableton-Live-bundled, JUCE-rendered) exposes no CLI/headless/UI-Automation path to
> trigger "Save As → Max for Live Device", so this last step is manual by nature.

`.amxd` is Max's encrypted/binary device format; it cannot be produced without a
Max/Max-for-Live installation. Steps (≈4 clicks each, patches need no editing):

1. In Max: **File → Open…** → `apps\max-for-live\flux-audio-bridge.maxpat`.
2. **File → Save As…** → set the file type to **Max Audio Effect** (a.k.a. Max for Live
   Audio Effect) → filename `flux-audio-bridge` → save into `apps\max-for-live\`.
3. **File → Open…** → `apps\max-for-live\flux-midi-bridge.maxpat`.
4. **File → Save As…** → type **Max MIDI Effect** → `flux-midi-bridge` → same folder.
5. Verify (PowerShell):
   ```powershell
   Get-ChildItem "D:\CURSOR LOCAL PROJECTS\VIDEO - GENFX\apps\max-for-live\*.amxd"
   ```
   Each `.amxd` should begin with the Max binary magic (`ampf`), not `{` (JSON).

If your Max build offers no device type in Save As, it is the standalone Max without the
Max-for-Live device authoring — use the Max bundled with Ableton Live 12 Suite instead.

**What is verified here vs. external:** the source patches (now pre-wired), the OSC
encode/decode contract, and `check-maxpat.cjs` conformance are verified now. Producing the
`.amxd` bytes and an in-host load test requires the Max GUI above — the recorded external
step.

