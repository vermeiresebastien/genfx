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

`.amxd` is Max's encrypted/binary device format; it cannot be produced without a
Max/Max-for-Live installation. The complete, verified source is here; the packaging
procedure is fully specified so it is reproducible on any machine with Max:

1. **Prerequisite:** Max 9 (or a Max for Live host) installed.
2. Verify the source is structurally valid and protocol-conformant:
   ```
   node apps/max-for-live/check-maxpat.cjs      # must print "maxpat structural checks: PASS"
   ```
3. Open each `.maxpat` in Max. Wire the `node.script bridge-osc.js` object into the
   patch (the encoder is provided; see the patch comments for the inlet layout).
4. Save each device as a **Max Audio Effect** (audio bridge) / **Max MIDI Effect**
   (MIDI bridge): *File → Save As → Max for Live Device* (`.amxd`).
5. Set the device **author** and **description** metadata to `Flux` and
   "Flux <audio|midi> bridge — OSC transport/feature sync".
6. Confirm the `.amxd` loads: drop it on a track in Live, verify it appears active
   and that `udpsend 127.0.0.1 9000` transmits on note/transport activity.
7. Record the Max version used in `implementation/evidence/`.

**What is verified here vs. external:** the source patches, OSC encode/decode
contract, and `check-maxpat.cjs` conformance are verified now. The `.amxd` bytes and
in-host load test require the Max host above and are the recorded external blocker.

