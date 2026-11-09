# Blockers

## External prerequisites (cannot be supplied by this run)
- **NDI SDK/runtime** (R006, G185, AT060): no authorized official SDK or licensed runtime
  installed. Adapter interface + loader implemented; real send/receive verification is
  `blocked_external`.
- **Spout2 / Syphon** (R006, AT058/AT059): Spout requires the Spout2 native SDK build;
  Syphon is macOS-only and no macOS host exists here. Interface implemented; runtime
  verification blocked.
- **Ableton Link license** (R015, AT063): no license decision; Link adapter is not enabled.
  Max/MIDI transport forwarding implemented separately.
- **Max for Live host** (R002, AT062): no Max/Live install; `.maxpat` sources are produced
  and structurally tested, `.amxd` packaging is blocked until a host exists.
- **macOS hosts** (R001, AT083/AT084): no macOS environment; all macOS-only code paths are
  `implemented_unverified`.
- **Code signing / notarization**: no signing identity configured.
- **Optional vision models** (G170–G177, AT075): no models/runtimes present; conditional
  nodes report MODEL_MISSING and stay editable.

## Documented deviations
- Desktop shell is a native egui/wgpu editor rather than Tauri+React (see D001). The
  engine, commands, validation and determinism contracts are unchanged; React Flow is
  replaced by an equivalent native node canvas.
- Media uses an FFmpeg process worker rather than in-process `ffmpeg-next` (see D004).
