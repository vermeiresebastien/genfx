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
