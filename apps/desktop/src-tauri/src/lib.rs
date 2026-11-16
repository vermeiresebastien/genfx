//! Flux desktop shell (Tauri 2).
//!
//! Rust is the authority: this shell exposes the engine's typed command surface
//! (`flux_app::commands::CommandHost`) over Tauri IPC. The webview only submits
//! requests and renders snapshots + preview frames; it never runs an independent
//! simulation or renderer.

use flux_app::commands::{CommandHost, GuiCommand, Request, Response};
use flux_app::control::OscBridge;
use flux_app::export::{export_project_with_frames, ExportOptions, PreviewRenderer};
use flux_app::producer::produce_node;
use flux_domain::{Project, RationalTime};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

struct AppState {
    host: Arc<Mutex<CommandHost>>,
    /// Cached GPU preview renderer (created lazily on first preview so a missing
    /// GPU doesn't break app startup). Reusing it avoids recreating the device
    /// and recompiling shaders on every frame.
    preview: Mutex<Option<PreviewRenderer>>,
    /// True while a background export is running (prevents concurrent GPU work).
    exporting: Arc<AtomicBool>,
    /// The live-control listener, and why it is not running if it is not.
    osc: Mutex<OscRuntime>,
}

/// Owned by this process, not by the webview: a listener in the UI layer would
/// put control authority on the wrong side of the IPC boundary.
struct OscRuntime {
    port: u16,
    bridge: Option<OscBridge>,
    bind_error: Option<String>,
}

/// The UDP port live control listens on.
///
/// `FLUX_OSC_PORT` moves it, which matters because the port has exactly one
/// owner and `flux osc <port>` uses the same default of 9000.
fn osc_port() -> u16 {
    let default = flux_integrations::osc::DEFAULT_RECV_PORT;
    match std::env::var("FLUX_OSC_PORT") {
        Ok(raw) => raw.trim().parse::<u16>().unwrap_or_else(|_| {
            eprintln!(
                "FLUX_OSC_PORT={:?} is not a port number; listening on {default}",
                raw
            );
            default
        }),
        Err(_) => default,
    }
}

/// Take a mutex whose previous holder may have panicked inside it.
///
/// Returns the guard, plus whether the mutex was poisoned.
///
/// Why this exists rather than `.lock().unwrap()`: `std::sync::Mutex` keeps the
/// poison flag for the life of the mutex, so one panic while a lock is held makes
/// *every later acquisition* panic as well. Both mutexes here guard long-lived
/// state — `host` is the document, `preview` the GPU renderer — so the first bad
/// preview frame would end every subsequent command and every later frame too:
/// the app is dead while its window is still showing the last image. Recovering
/// makes one failed frame a failed frame instead.
///
/// What recovery does NOT assert is that the guarded value is consistent. A
/// `PoisonError` says only that a panic crossed the critical section, and nothing
/// in this process can prove what it left behind. So each caller answers for
/// itself, and the three answers it may choose from are stated here rather than
/// implied there:
///  * *continue* — the value is only read out, and one stale read is not a
///    corruption (the `host` project clone in the render commands, `osc_status`);
///  * *discard* — the guarded value is reconstructible from something we do
///    trust, so throwing it away is the honest repair (`preview_disposition`: the
///    renderer's `roots_cache: HashMap<String, RuntimeGraph>` is rebuilt by
///    rendering the project again, which is why a poisoned preview can be answered
///    safely and a poisoned document cannot);
///  * *refuse* — neither answer is defensible, so the call says so with a reason
///    the UI can show (`export_disposition`: starting an export while the state of
///    the one GPU device already created is unknown).
///
/// What this cannot tell any of them is *what panicked*. `PoisonError` hands back
/// the guard and nothing else — its payload is a private field, not a method
/// (measured: `error[E0599]: no method named `data` found for struct
/// `PoisonError<T>` ... private field, not a method`), and the only stable routes
/// to a panic message are a process-wide `set_hook` or the `JoinHandle` of the
/// panicking thread, neither of which is bound to *this* lock. So the reasons
/// built below name the lock and the action taken, and never quote a panic this
/// process did not observe.
///
/// What recovery must NOT do, however, is *clear* the flag.
/// `Mutex::clear_poison` is the API that asserts the guarded value is sound, and
/// this file licenses that assertion in exactly two places — both inside
/// `preview_guard`, both one line after the value the panic crossed has either
/// stopped existing or been replaced by one this process just built. The `host` and
/// `osc` mutexes are never cleared: for those the answer is *continue* precisely
/// because the document cannot be vouched for, so erasing the flag there would
/// delete the only record a later reader has. That is gated by name, in
/// `clear_poison_is_called_only_where_the_value_is_gone_or_new`.
///
/// `what` names the mutex for the stderr line; a caller that shows the user a
/// reason uses the `*_disposition` functions, so the two cannot disagree.
fn lock_recovered<'a, T>(m: &'a Mutex<T>, what: &str) -> (MutexGuard<'a, T>, bool) {
    match m.lock() {
        Ok(guard) => (guard, false),
        Err(err) => {
            eprintln!(
                "flux-desktop: {what} was poisoned by a panic while held; recovering the guard"
            );
            (err.into_inner(), true)
        }
    }
}

/// What a poisoned preview lock means for the renderer it guards.
///
/// Split out from `PreviewRenderer` on purpose: the policy is about an `Option`,
/// so it can be gated with a sentinel slot on a host with no GPU, and the three
/// legs below are the whole rule. `T` stands in for the renderer at the gate.
///
///  1. not poisoned — nothing to decide, the slot may be used as it is;
///  2. poisoned and the slot still holds a renderer — discard it and refuse THIS
///     frame. The renderer is the thing we cannot vouch for: a panic inside
///     `render_graph` can cross a `wgpu` submission and leave the cached
///     `roots_cache: HashMap<String, RuntimeGraph>` half-updated, and a frame
///     rendered from a half-updated graph looks exactly like a frame rendered
///     from a whole one. Refusing is cheaper than being wrong, and dropping the
///     renderer is the repair we can prove, because a `PreviewRenderer` is
///     rebuilt from the project on the next call;
///  3. poisoned but the slot is already empty — proceed. This is the "once" in
///     "refuse once", and it is not a convenience: `std::sync::Mutex` never
///     clears its poison flag, so a rule keyed on the *flag* would refuse every
///     frame for the rest of the session, which is the dead-app defect arriving
///     by another door. The danger is the torn renderer, not the bit that
///     recorded how it got dropped; once the renderer is gone there is nothing
///     left to distrust, and the next call initialises a fresh one.
///
/// What this function does NOT do is clear the flag — keyed on the slot as it is,
/// it cannot know whether the value it just dropped was the last hazard. That is
/// `preview_guard`'s job, one line after this returns; the session claim is gated
/// there and not here, because the policy alone was gated once already and it was
/// not enough.
fn preview_disposition<T>(poisoned: bool, slot: &mut Option<T>) -> Result<(), String> {
    if !poisoned {
        return Ok(());
    }
    if slot.is_none() {
        return Ok(());
    }
    *slot = None;
    Err(
        "preview discarded after a panic crossed the GPU renderer's lock; this frame was not \
         rendered, and the renderer will be rebuilt on the next one"
            .to_string(),
    )
}

/// Whether an export may start while the preview lock is in this state.
///
/// Same three legs as `preview_disposition`, opposite second answer — and the
/// difference is the point. The preview path can throw the renderer away because
/// it is about to rebuild it. The export path must not: deciding who owns the one
/// GPU device is defect 6's question, not this item's, and dropping the renderer
/// from here would answer it implicitly. So this arm refuses, leaves the renderer
/// exactly as it was found, and says what to do about it.
///
///  1. not poisoned — release the device and start, as before;
///  2. poisoned and the slot holds a renderer — refuse. A second device started on
///     top of one whose state is unknown is exactly the driver crash the release
///     step exists to avoid, and the poisoned lock is the reason the state is
///     unknown;
///  3. poisoned and the slot is empty — start. There is no device left to
///     collide with, and the poison bit says only how the renderer was dropped,
///     not that anything is still held. This is the leg that makes the refusal in
///     leg 2 a one-time refusal rather than a permanent one: the user renders a
///     preview frame, that frame discards the renderer, and the next export goes.
fn export_disposition(poisoned: bool, slot_holds_renderer: bool) -> Result<(), String> {
    if !poisoned || !slot_holds_renderer {
        return Ok(());
    }
    Err(
        "export refused: the preview renderer was left behind by a panic and still holds its GPU \
         device, and starting a second device on top of one whose state is unknown is the driver \
         crash this path exists to avoid. Render one preview frame first — that discards the \
         poisoned renderer — then export."
            .to_string(),
    )
}

/// The preview-frame composition: acquire, decide, and refill.
///
/// Generic over the renderer on purpose, and the commands reach it through
/// `preview_renderer` with the real constructor. The reason it is a function and
/// not five lines inside `preview_renderer` is the defect this file now carries a
/// gate for: the policy was tested against a hand-seeded slot, while the caller
/// that uses the policy *refills* the slot on every accepted frame, so "refuses
/// once" was true of the policy and false of the session. A gate has to replay the
/// composition, and a composition that is copied into a test drifts from the one
/// that ships. `init` runs only when the slot is empty, and its error becomes the
/// caller's reason (`gpu init: …` at the real site).
///
/// Never panics, and never hands back an empty slot: a caller that cannot render
/// says so in words the webview can show.
fn preview_guard<'a, G>(
    preview: &'a Mutex<Option<G>>,
    what: &str,
    init: impl FnOnce() -> Result<G, String>,
) -> Result<MutexGuard<'a, Option<G>>, String> {
    let (mut guard, poisoned) = lock_recovered(preview, what);
    if let Err(reason) = preview_disposition(poisoned, &mut guard) {
        // Licensed clear #1 of two, and the licence is that the value the panic
        // crossed has just stopped existing on the line above. `preview_disposition`
        // drops the renderer it found, so what the slot holds now is `None` — which
        // cannot be torn — and the `PreviewRenderer` with the possibly-half-updated
        // `roots_cache` is gone with it. `clear_poison` is the API that asserts "the
        // guarded data is not corrupt", and this is the one moment in a preview
        // frame where that assertion is *mine* to make rather than hoped for.
        //
        // Skipping BOTH clear sites is the reopened defect, measured: the flag
        // survives, the next accepted frame builds a fresh renderer on a permanently
        // poisoned mutex, the frame after that meets `(poisoned, Some)` and refuses
        // again — 6 refusals and 5 rebuilds per single poison over 11 frames
        // (`one_poison_with_a_renderer_held_refuses_exactly_one_frame`,
        // smoke/I7-1-mut-q1.log).
        //
        // What this site buys on its own is narrower than the sequence gate, and the
        // measurement says so: delete only this line and every count in the sequence
        // gates is unchanged (smoke/I7-1-mut-q2.log prints the same 1 refusal / 1
        // rebuild as the green run), because site #2 clears the flag at the very next
        // rebuild — one frame later. So what this site actually shortens is the window
        // in which the flag lies: with it, the mutex is clean the moment the refusal is
        // returned; without it, the poison survives until something else rebuilds, and
        // if the rebuild never succeeds (a `gpu init` error propagates out before
        // site #2) it survives the rest of the session. It is pinned by the call-site
        // gate, not by a count.
        //
        // The claim is narrow on purpose: it is about this mutex's value and nothing
        // else. It says nothing about the wgpu device the dropped renderer held or
        // about the shared `RuntimeGraph`, which are defect 6's and lane B3's.
        preview.clear_poison();
        return Err(reason);
    }
    if guard.is_none() {
        *guard = Some(init()?);
        // Licensed clear #2 of two, for the case where no refusal ever happens: the
        // panic can cross this lock while the slot is empty — which is the ordinary
        // state right after an export released the device — so the site above is
        // never reached, and a healthy renderer would otherwise inherit a poisoned
        // mutex and get refused one frame later for nothing. Here the licence is that
        // the value the flag described is absent and what the slot holds now was
        // constructed by this call, on the line above.
        //
        // This is the site that carries the fix. Deleting it alone is falsified by
        // measurement, not by argument: the poison survives the rebuild, so frame 2
        // meets `(poisoned, Some)` and refuses a renderer that this very function just
        // built — 1 refusal and 2 refills over 5 frames where the green run says 0 and
        // 1 (smoke/I7-1-mut-q3.log, against
        // `one_poison_with_an_empty_slot_refuses_no_frame_at_all`).
        //
        // Not reached when `init` itself fails: the `?` returns before this line, so a
        // session whose GPU never comes back keeps the flag set. That clear would be
        // licensed (the slot is empty, so the value the flag described is absent) but no
        // measured behaviour needs it, and a constructor that returned `Err` is the one
        // place in this file where we do not know what it touched — so the flag stays.
        preview.clear_poison();
    }
    // The slot cannot be empty here — `preview_disposition` either kept it or
    // refused, and the line above filled it. Callers still go through
    // `preview_slot`, because the alternative was `unwrap()`.
    Ok(guard)
}

/// `preview_guard` with the thing the desktop actually guards.
fn preview_renderer(
    preview: &Mutex<Option<PreviewRenderer>>,
) -> Result<MutexGuard<'_, Option<PreviewRenderer>>, String> {
    preview_guard(preview, "preview renderer", || {
        PreviewRenderer::new().map_err(|e| format!("gpu init: {e:?}"))
    })
}

/// The preview half of an export: decide, and when the answer is *start*, release
/// the device by emptying the slot.
///
/// Generic, and extracted from `export_project_cmd`, for the same reason
/// `preview_guard` is: the export claim below is a *sequence* claim, and a sequence
/// can only be replayed against a function. The `exporting` flag stays the
/// command's business — it owns the reset on this `Err`, because a refusal that
/// left the flag set would wedge every later export too.
///
/// Deliberately does NOT drop the renderer on the refusal path, and does not clear
/// the poison anywhere: dropping it would decide GPU-device ownership from the
/// export side, which is defect 6's question and lane B3's work, and clearing here
/// would assert that a value this function did not examine is sound.
fn export_preview_release<G>(preview: &Mutex<Option<G>>, what: &str) -> Result<(), String> {
    let (mut guard, poisoned) = lock_recovered(preview, what);
    export_disposition(poisoned, guard.is_some())?;
    // Either the lock was clean, or it was poisoned and an earlier refusal had
    // already dropped the renderer; in both cases there is no device left to
    // collide with, so the export may have it.
    *guard = None;
    Ok(())
}

/// The renderer the caller asked for, or the reason there is none. This is now
/// the only way a preview command reaches the renderer: the old form was
/// `guard.as_mut().unwrap()`, which turned an empty slot into a panic inside a
/// Tauri command — the webview's promise never settles, the window keeps showing
/// the previous frame, and nothing in the UI says why.
///
/// Two gates, because one cannot do the other's job. This function is generic
/// over the slot so both of its arms are gated as behaviour
/// (`an_empty_preview_slot_answers_with_a_showable_reason`, no GPU needed); that
/// it is never *reached* with an empty slot is a property of
/// `preview_renderer`'s initialisation, which needs a device, so it is gated as a
/// ban on the panic form (`no_command_unwraps_a_lock_or_a_slot`) instead. A
/// behaviour test for the unreachable arm would only be proving the
/// initialisation is wrong.
fn preview_slot<T>(slot: &mut Option<T>) -> Result<&mut T, String> {
    slot.as_mut()
        .ok_or_else(|| "no preview renderer: this frame was not rendered".to_string())
}

#[tauri::command]
fn dispatch(state: tauri::State<AppState>, request: Request) -> Response {
    // Continue, and say so on stderr. `Response` already carries a typed
    // `error` slot for the *request*, and putting a mutex-recovery note in it
    // would make a successful command look failed — that is a wire-format
    // decision, not this item's. The frame that panicked is already lost; this
    // one is answered.
    let (mut host, _) = lock_recovered(&state.host, "command host");
    host.handle(request)
}

#[tauri::command]
fn open_monitor(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("monitor") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "monitor", WebviewUrl::App("monitor.html".into()))
        .title("Flux Preview")
        .inner_size(960.0, 540.0)
        .min_inner_size(320.0, 180.0)
        .resizable(true)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_monitor_fullscreen(app: tauri::AppHandle) -> Result<bool, String> {
    let window = app
        .get_webview_window("monitor")
        .ok_or_else(|| "monitor window is not open".to_string())?;
    let fullscreen = !window.is_fullscreen().map_err(|e| e.to_string())?;
    window
        .set_fullscreen(fullscreen)
        .map_err(|e| e.to_string())?;
    Ok(fullscreen)
}

/// Render a single preview frame of the active project at time `t` (seconds)
/// at reduced resolution; returns a PNG data URL for the webview.
#[tauri::command]
fn render_preview(
    state: tauri::State<AppState>,
    time_seconds: f64,
    width: u32,
    height: u32,
) -> Result<String, String> {
    let (project, graph) = {
        // Read out and drop: the host lock is never held into the GPU path, so a
        // panic in `render_graph` cannot poison the document as well as the
        // renderer. Recovery here continues, because what this reads is a clone
        // the caller is about to render, not a claim about the document.
        let (host, _) = lock_recovered(&state.host, "command host (preview read)");
        let g = host
            .dispatcher
            .project
            .graphs
            .get(&host.active_graph)
            .cloned();
        (host.dispatcher.project.clone(), g)
    };
    let t = RationalTime::new((time_seconds * 1_000_000.0) as i64, 1_000_000)
        .map_err(|e| e.to_string())?;
    let mut guard = preview_renderer(&state.preview)?;
    let renderer = preview_slot(&mut guard)?;
    // Render the node-graph editor's active graph (fall back to timeline render
    // if the project has no graph).
    let rgba = match &graph {
        Some(g) => renderer.render_graph(g, &project, t, width, height),
        None => renderer.render(&project, t, width, height),
    }
    .map_err(|e| format!("render: {e:?}"))?;
    // Encode the RGBA to PNG in memory (no disk round-trip).
    let png =
        flux_media::encode_png_rgba(width, height, &rgba).map_err(|e| format!("png: {e:?}"))?;
    if png.len() > 4 * 1024 * 1024 {
        return Err("preview too large".into());
    }
    Ok(format!("data:image/png;base64,{}", b64(&png)))
}

fn b64(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Nearest-neighbour downscale of packed RGBA for the live export preview.
fn downscale_rgba(src: &[u8], w: u32, h: u32, max_w: u32) -> (u32, u32, Vec<u8>) {
    if w == 0 || h == 0 || w <= max_w {
        return (w, h, src.to_vec());
    }
    let dw = max_w;
    let dh = (((h as u64) * (dw as u64) + (w as u64) / 2) / (w as u64)).max(1) as u32;
    let mut out = vec![0u8; (dw as usize) * (dh as usize) * 4];
    let need = (dw as usize) * (dh as usize) * 4;
    if src.len() < need {
        return (w, h, src.to_vec());
    }
    for y in 0..dh {
        let sy = (y as u64 * h as u64 / dh as u64) as u32;
        for x in 0..dw {
            let sx = (x as u64 * w as u64 / dw as u64) as u32;
            let si = ((sy as usize) * (w as usize) + (sx as usize)) * 4;
            let di = ((y as usize) * (dw as usize) + (x as usize)) * 4;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    (dw, dh, out)
}

/// Start an export on a background thread. Progress, live frames and completion
/// are reported through events (`export-progress`, `export-frame`, `export-done`,
/// `export-error`) so the webview can show the real output while rendering.
#[tauri::command]
fn export_project_cmd(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    out_dir: String,
    width: u32,
    height: u32,
    codec: Option<String>,
    keep_frames: Option<bool>,
) -> Result<String, String> {
    let project = {
        let (host, _) = lock_recovered(&state.host, "command host (export read)");
        host.dispatcher.project.clone()
    };
    if state.exporting.swap(true, Ordering::SeqCst) {
        return Err("an export is already running".into());
    }
    let exporting = state.exporting.clone();
    // Release the live preview GPU device before exporting so only one device is
    // active at a time (two concurrent wgpu devices can crash some drivers).
    //
    // The `if let Ok(mut guard)` this replaces is the defect in this item's own
    // shape: on a poisoned preview lock it skipped the release, said nothing and
    // started the export anyway, so the one-device guarantee was traded for a
    // green "export started" exactly when the state of that device was least
    // knowable. Refusing is the honest answer. And the flag goes back, because a
    // refusal that left `exporting` set would wedge every later export as well —
    // the same permanent-dead shape this item exists to remove.
    //
    // What is deliberately NOT done here: dropping the renderer, which is what
    // the preview path does. That would answer who owns the device from the
    // export path, and it would be an implicit fix for defect 6 — the renderer is
    // left exactly as it was found, held by a lock nobody can vouch for, and the
    // export does not start. Naming that owner is a separate decision with its
    // own gate; this arm only refuses, and says what to do about it.
    if let Err(reason) = export_preview_release(&state.preview, "preview renderer (export)") {
        exporting.store(false, Ordering::SeqCst);
        return Err(reason);
    }
    // Resolve relative output dirs against the process CWD so the reported path
    // is absolute and the user can find the file.
    let out_dir = {
        let p = std::path::PathBuf::from(&out_dir);
        if p.is_absolute() {
            p
        } else {
            std::env::current_dir().map(|d| d.join(&p)).unwrap_or(p)
        }
    };
    let codec = flux_app::export::codec_from_name(codec.as_deref().unwrap_or("h264"));
    let keep = keep_frames.unwrap_or(false);

    std::thread::spawn(move || {
        let progress_app = app.clone();
        let progress: flux_app::export::ProgressFn = Box::new(move |done, total| {
            let _ = progress_app.emit(
                "export-progress",
                serde_json::json!({ "done": done, "total": total }),
            );
        });
        let opts = ExportOptions {
            out_dir,
            project_name: project.name.clone(),
            codec,
            width,
            height,
            range: None,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            feature_source: flux_domain::params::FeatureSource::DocumentOnly,
        };
        // Stream the actual export frames to the monitor while rendering. The UI
        // pauses its own preview loop during export, so only one GPU device is
        // doing work at a time.
        let frame_app = app.clone();
        let last_emit = std::cell::Cell::new(std::time::Instant::now());
        let frame_cb: flux_app::export::FrameFn = Box::new(move |done, total, rgba, w, h| {
            if done < total && last_emit.get().elapsed().as_millis() < 90 {
                return;
            }
            last_emit.set(std::time::Instant::now());
            let (dw, dh, small) = downscale_rgba(rgba, w, h, 480);
            if let Ok(png) = flux_media::encode_png_rgba(dw, dh, &small) {
                let _ = frame_app.emit(
                    "export-frame",
                    serde_json::json!({
                        "dataUrl": format!("data:image/png;base64,{}", b64(&png)),
                        "done": done,
                        "total": total,
                    }),
                );
            }
        });
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            export_project_with_frames(&project, &opts, Some(progress), Some(frame_cb))
        }));
        match outcome {
            Ok(Ok(res)) => {
                exporting.store(false, Ordering::SeqCst);
                // A PNG sequence is an implementation detail of encoding; remove it
                // unless the caller explicitly asked to keep it (or there is no video).
                if !keep && res.video.is_some() {
                    let _ = std::fs::remove_dir_all(&res.frames_dir);
                }
                let _ = app.emit(
                    "export-done",
                    serde_json::json!({
                        "frames": res.frames,
                        "samples": res.samples,
                        "video": res.video.as_ref().map(|p| p.display().to_string()),
                        "wav": res.wav.as_ref().map(|p| p.display().to_string()),
                        "framesDir": if keep || res.video.is_none() {
                            Some(res.frames_dir.display().to_string())
                        } else {
                            None
                        },
                    }),
                );
            }
            Ok(Err(e)) => {
                exporting.store(false, Ordering::SeqCst);
                let _ = app.emit(
                    "export-error",
                    serde_json::json!({ "message": format!("{e:?}") }),
                );
            }
            Err(_) => {
                exporting.store(false, Ordering::SeqCst);
                let _ = app.emit(
                    "export-error",
                    serde_json::json!({ "message": "export thread panicked" }),
                );
            }
        }
    });

    Ok("export started".into())
}

#[tauri::command]
fn capabilities() -> serde_json::Value {
    serde_json::json!({
        "capture": flux_platform::capture_capabilities(),
        "integrations": flux_integrations::capability_report(),
    })
}

/// What the live-control listener knows. Every packet it accepts lands in one
/// counter, so a fader that appears to do nothing is a number to read rather
/// than a mystery: `unmapped` means the binding table or the node, `notApplied`
/// means the engine has no target for that kind yet, `droppedByReceiver` means
/// the stream outran the rate limiter.
#[tauri::command]
fn osc_status(state: tauri::State<AppState>) -> serde_json::Value {
    // Continue: every field below is a read-out of what the listener recorded,
    // and a poison here means a panic happened somewhere else while this lock
    // happened to be held — answering with the numbers is not a new claim.
    let (g, _) = lock_recovered(&state.osc, "osc runtime");
    serde_json::json!({
        "port": g.port,
        "bound": g.bridge.is_some(),
        "bindError": g.bind_error,
        "stats": g.bridge.as_ref().map(|b| b.stats()).unwrap_or(serde_json::Value::Null),
    })
}

/// What the live audio capture knows: the last measurement, whether it can be
/// trusted, and the reason there is no measurement at all. The capture is owned
/// by the `CommandHost` that `SetLiveAudio` starts it on, so this reads the same
/// slot the command returns — the webview gets a read-out, not a second owner.
///
/// `live-audio` is not in the default build, so in this binary the honest answer
/// is `compiled: false`, said plainly rather than as a level of zero.
#[tauri::command]
fn live_audio_status(state: tauri::State<AppState>) -> serde_json::Value {
    // Continue, for the same reason as `osc_status`: this reads the slot the
    // capture command writes, and the read-out is a report, not a repair.
    let (host, _) = lock_recovered(&state.host, "command host (live-audio read)");
    host.live_audio_status()
}

#[tauri::command]
fn produce_generator_frame(
    generator_id: String,
    width: u32,
    height: u32,
    time_seconds: f64,
) -> Result<String, String> {
    // Direct producer preview for a generator type (used by the library peek).
    let rgba = produce_node(
        &generator_id,
        &Default::default(),
        width,
        height,
        time_seconds,
        1,
    )
    .map_err(|e| format!("{e:?}"))?;
    match rgba {
        Some(_) => Ok("producer".into()),
        None => Ok("analytic".into()),
    }
}

/// Render a graph to raw RGBA bytes (no PNG encode, no base64). Used for live
/// playback: the webview paints this into a canvas, which is far cheaper than a
/// PNG encode + data-URL decode per frame.
///
/// Returns the bytes as a binary IPC response (`ArrayBuffer` in JS). Returning a
/// `Vec<u8>` normally serialises to a JSON array of numbers — ~278 000 numbers
/// for a 352x198 frame — which was the real cause of choppy previews.
#[tauri::command]
fn render_preview_raw(
    state: tauri::State<AppState>,
    graph_id: String,
    time_seconds: f64,
    width: u32,
    height: u32,
    output_node_id: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    let (project, mut graph) = {
        let (host, _) = lock_recovered(&state.host, "command host (graph read)");
        (
            host.dispatcher.project.clone(),
            host.dispatcher.project.graphs.get(&graph_id).cloned(),
        )
    };
    let mut graph = graph.take().ok_or_else(|| "graph not found".to_string())?;
    if let Some(node_id) = output_node_id {
        if graph.node(&node_id).is_none() {
            return Err("output node not found".into());
        }
        graph.output_node = node_id;
    }
    let t = RationalTime::new((time_seconds * 1_000_000.0) as i64, 1_000_000)
        .map_err(|e| e.to_string())?;
    let mut guard = preview_renderer(&state.preview)?;
    let renderer = preview_slot(&mut guard)?;
    let rgba = renderer
        .render_graph(&graph, &project, t, width, height)
        .map_err(|e| format!("render: {e:?}"))?;
    Ok(tauri::ipc::Response::new(rgba))
}

#[tauri::command]
fn build_info() -> serde_json::Value {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "gitHash": option_env!("FLUX_GIT_HASH").unwrap_or("unknown"),
    })
}

/// Render a chosen graph (or a specific node within it) to a PNG data URL.
/// Used by the Output node's inline thumbnail and the detached preview monitor;
/// `output_node_id` overrides which node is treated as the result, so each Output
/// node on the canvas can preview its own subgraph.
#[tauri::command]
fn render_graph_preview(
    state: tauri::State<AppState>,
    graph_id: String,
    time_seconds: f64,
    width: u32,
    height: u32,
    output_node_id: Option<String>,
) -> Result<String, String> {
    let (project, mut graph) = {
        let (host, _) = lock_recovered(&state.host, "command host (graph read)");
        (
            host.dispatcher.project.clone(),
            host.dispatcher.project.graphs.get(&graph_id).cloned(),
        )
    };
    let mut graph = graph.take().ok_or_else(|| "graph not found".to_string())?;
    if let Some(node_id) = output_node_id {
        if graph.node(&node_id).is_none() {
            return Err("output node not found".into());
        }
        graph.output_node = node_id;
    }
    let t = RationalTime::new((time_seconds * 1_000_000.0) as i64, 1_000_000)
        .map_err(|e| e.to_string())?;
    let mut guard = preview_renderer(&state.preview)?;
    let renderer = preview_slot(&mut guard)?;
    let rgba = renderer
        .render_graph(&graph, &project, t, width, height)
        .map_err(|e| format!("render: {e:?}"))?;
    let png =
        flux_media::encode_png_rgba(width, height, &rgba).map_err(|e| format!("png: {e:?}"))?;
    if png.len() > 4 * 1024 * 1024 {
        return Err("preview too large".into());
    }
    Ok(format!("data:image/png;base64,{}", b64(&png)))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Single instance: a second launch would open its own WebView2 profile and
    // both would race on the GPU device + user data, which is exactly how a
    // stale process kept serving an old inspector.
    if !try_acquire_single_instance() {
        std::process::exit(0);
    }

    let host = Arc::new(Mutex::new(CommandHost::new(Project::new("Untitled", 1))));
    let port = osc_port();
    tauri::Builder::default()
        .manage(AppState {
            host,
            preview: Mutex::new(None),
            exporting: Arc::new(AtomicBool::new(false)),
            osc: Mutex::new(OscRuntime {
                port,
                bridge: None,
                bind_error: None,
            }),
        })
        // Start listening for Live. A failure here is recorded and reported by
        // `osc_status`, never swallowed: an app that silently ignores the faders
        // is indistinguishable from one that has no listener at all.
        .setup(|app| {
            let app_handle = app.handle().clone();
            let state = app.state::<AppState>();
            // At startup the mutex has just been built and cannot be poisoned, so
            // `lock_recovered` is a no-op on every one of these three acquisitions.
            // It is used anyway, so that the ban in
            // `no_command_unwraps_a_lock_or_a_slot` can be absolute instead of
            // "everywhere except the lines I decided were safe".
            let runtime_port = lock_recovered(&state.osc, "osc runtime (setup)").0.port;
            let emit = move |params: &[String], revision: u64| {
                let _ = app_handle.emit(
                    "live-control",
                    serde_json::json!({ "params": params, "revision": revision }),
                );
            };
            match OscBridge::start(runtime_port, state.host.clone(), emit) {
                Ok(bridge) => {
                    let (mut runtime, _) = lock_recovered(&state.osc, "osc runtime (bridge)");
                    runtime.bridge = Some(bridge);
                }
                Err(e) => {
                    eprintln!("flux-desktop: OSC listener could not bind port {runtime_port}: {e}");
                    let (mut runtime, _) = lock_recovered(&state.osc, "osc runtime (bind error)");
                    runtime.bind_error = Some(e.to_string());
                }
            }
            Ok(())
        })
        // The detached monitor is owned by the main editor window. Close it
        // before the editor exits so WebView2 does not leave a helper process
        // holding the Flux user-data profile open.
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, WindowEvent::CloseRequested { .. }) {
                if let Some(monitor) = window.app_handle().get_webview_window("monitor") {
                    let _ = monitor.close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            dispatch,
            open_monitor,
            toggle_monitor_fullscreen,
            render_preview,
            render_graph_preview,
            render_preview_raw,
            build_info,
            export_project_cmd,
            capabilities,
            osc_status,
            live_audio_status,
            produce_generator_frame
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            match event {
                RunEvent::ExitRequested { .. } => {
                    // Close every owned webview before the runtime tears down.
                    // This makes normal shutdown deterministic even when the
                    // detached preview monitor is still open.
                    for window in app.webview_windows().values() {
                        let _ = window.close();
                    }
                }
                RunEvent::Exit => {
                    // WebView2 can keep helper processes alive after a window
                    // closes. Terminate only this Flux process tree so the
                    // detached preview and its renderer cannot outlive Flux.
                    #[cfg(target_os = "windows")]
                    {
                        let pid = std::process::id().to_string();
                        let _ = std::process::Command::new("taskkill")
                            .args(["/PID", &pid, "/T", "/F"])
                            .spawn();
                    }
                }
                _ => {}
            }
        });
}

fn _uses(_: GuiCommand) {}

/// Hold a named mutex for the life of the process so only one Flux instance runs
/// at a time. Returns false if another instance already holds it.
#[cfg(target_os = "windows")]
fn try_acquire_single_instance() -> bool {
    use std::ptr::null_mut;

    const ERROR_ALREADY_EXISTS: u32 = 183;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(
            lp_mutex_attributes: *mut std::ffi::c_void,
            b_initial_owner: i32,
            lp_name: *const u16,
        ) -> *mut std::ffi::c_void;
        fn GetLastError() -> u32;
        fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
    }

    let name: Vec<u16> = "Local\\dev.flux.desktop.single"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe { CreateMutexW(null_mut(), 1, name.as_ptr()) };
    if handle.is_null() {
        return true; // cannot determine; never hard-block startup
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { CloseHandle(handle) };
        return false;
    }
    // Intentionally leak the handle: it must stay open for the whole process so
    // the mutex is released on exit.
    std::mem::forget(handle as usize);
    true
}

#[cfg(not(target_os = "windows"))]
fn try_acquire_single_instance() -> bool {
    true
}

#[cfg(test)]
mod tests {
    /// `build.rs` is the one place the webview bundle is produced, and a test
    /// cannot run it: cargo has already executed it before the test binary
    /// exists. So the invariant is read off the source, the way
    /// `capability_report_never_derives_verified_from_a_build_feature` reads the
    /// capability table. What these two gates cannot see is stated in the test
    /// that owns the shape: `smoke/I5-1-negative-child.log` (bundler exits
    /// nonzero -> `cargo build -p flux-desktop` exits 101) is the end-to-end
    /// proof, and the same probe against the previous `build.rs` exited 0.
    ///
    /// Ban 1: a bundle failure may never be reported as `cargo:warning` again.
    /// That is precisely how the defect shipped — both arms warned and fell
    /// through, so a green build proved nothing about which `app.bundle.js` was
    /// embedded, and the only trace was a line in
    /// `target/debug/build/flux-desktop-*/output`.
    #[test]
    fn ui_bundle_failure_is_never_reported_as_a_warning() {
        let script = include_str!("../build.rs");
        // Comment lines are excluded on purpose: build.rs *explains* the old
        // warning arms, so the string `cargo:warning` legitimately appears in
        // prose. What is banned is emitting it — a line of code.
        let warned: Vec<&str> = script
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with("//"))
            .filter(|line| line.contains("cargo:warning"))
            .collect();
        assert!(
            warned.is_empty(),
            "build.rs still emits an ungated warning: {warned:?}. A warning that the build \
             continues past is the defect this gate exists to keep out."
        );
        // Both arms must be *decisions about the child*, not prose about it: the
        // nonzero-exit arm and the spawn-failure arm each carry their own message,
        // and the exit status is inspected before anything is concluded.
        for needle in [
            "if !out.status.success()",
            "ui bundle failed",
            "could not start",
            "bundle_ui(&ui)",
        ] {
            assert!(
                script.contains(needle),
                "build.rs no longer contains {needle:?}: the bundle step is not gated the way \
                 item 1 left it (status inspected, both failure arms fatal, no `let _ =`)."
            );
        }
        assert!(
            !script.contains("let _ = bundle_ui"),
            "build.rs ignores the bundle step's result"
        );
    }

    /// Ban 2: every top-level UI source the bundler reads must be a *reason for
    /// cargo to re-run it*. At 63bc321 five were missing (`bundle.mjs`,
    /// `monitor.js`, `monitor.html`, `package.json`, `package-lock.json`), so
    /// editing the bundler or the monitor did not re-bundle at all — the build
    /// script never ran, and a green build embedded the old bundle. The gate
    /// enumerates the directory instead of restating the list, so a new UI file
    /// is caught the same way.
    #[test]
    fn every_ui_source_the_bundler_reads_is_watched() {
        let script = include_str!("../build.rs");
        let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui");
        let mut unwatched: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(&ui).expect("apps/desktop/ui is not readable") {
            let path = entry.expect("ui directory entry").path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .expect("ui file name")
                .to_string_lossy()
                .to_string();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            // `mjs` and `cjs` are listed explicitly: matching only `js` would
            // skip `bundle.mjs` — the single most important input here — because
            // a file extension is not a substring match. Measured: the first
            // version of this gate missed it and reported four unwatched files
            // where there are five (smoke/I5-1-mut-watchset.log).
            if !matches!(ext, "js" | "mjs" | "cjs" | "html" | "json") {
                continue;
            }
            // Two exemptions, both load-bearing:
            //  * `app.bundle.js` is this script's own *output*. Watching an output
            //    makes every build re-run the build script, which is the cycle that
            //    hid the rerun asymmetry at 63bc321 (measured: two consecutive
            //    `cargo build -p flux-desktop` both re-ran with nothing touched).
            //  * `check-*.mjs` are gate scripts (item 2's bundle check); they are
            //    not inputs to a bundle.
            if name == "app.bundle.js" || name.starts_with("check-") {
                continue;
            }
            let want = format!("cargo:rerun-if-changed=../ui/{name}");
            if !script.contains(&want) {
                unwatched.push(name);
            }
        }
        assert!(
            unwatched.is_empty(),
            "build.rs does not watch {unwatched:?} in apps/desktop/ui. Editing those files does \
             not re-run the bundler, so `cargo build -p flux-desktop` embeds the previously \
             built app.bundle.js and reports success."
        );
    }

    // --- item 3: one poisoned mutex must not end the app --------------------
    //
    // These legs never touch a GPU and never open a window: `preview_renderer`
    // and `export_project_cmd` were split into `lock_recovered` + a
    // `*_disposition` policy over an `Option` precisely so that the whole rule is
    // reachable from a `usize` and a sentinel slot. What cannot be gated here is
    // named in the last test of this block.

    use super::{
        export_disposition, export_preview_release, lock_recovered, preview_disposition,
        preview_guard, preview_slot, Arc, Mutex,
    };
    use std::cell::Cell;
    use std::rc::Rc;

    /// Poison `m` the only way poisoning happens: a panic unwinding through a
    /// held guard, on a thread that then dies of it.
    ///
    /// The `JoinHandle` is the evidence, and it is asserted rather than ignored:
    /// a leg that "recovers" from a mutex nobody ever poisoned would pass, so the
    /// helper refuses to return until it has proof that a panic crossed the
    /// critical section. (`thread::scope` was the first draft and is wrong here:
    /// it re-panics in the *test* thread when the scope ends, which is a second
    /// panic unrelated to the one under test.)
    ///
    /// Expect a `thread '...' panicked at 'gate: …'` line in the log while these
    /// tests run. It is not a failure: that panic *is* the symptom under test,
    /// and the legs below assert which acquisition survives it.
    ///
    /// Generic over the guarded value because the sequence gates poison both shapes
    /// a preview slot has — `Some(renderer)` and `None` — and the two are licensed
    /// differently at the recovery sites. The type is the only thing that changes;
    /// the two assertions below are the whole contract.
    fn poison_in_a_thread<T: Send + 'static>(m: &Arc<Mutex<T>>) {
        let holder = m.clone();
        let thread = std::thread::spawn(move || {
            let _held = holder.lock().unwrap();
            panic!("gate: panic while the lock is held");
        });
        assert!(
            thread.join().is_err(),
            "the poisoner exited cleanly, so nothing was ever poisoned — every leg that follows \
             would be recovery from a poison that does not exist"
        );
        assert!(
            m.is_poisoned(),
            "the thread panicked but the mutex is not poisoned: the precondition this gate is \
             built on does not hold on this toolchain"
        );
    }

    /// Did this closure panic? Answering the question rather than letting the
    /// harness fail on the panic is what lets one test compare the banned form
    /// and the recovery form against the *same* poisoned mutex.
    fn did_panic<F: FnOnce()>(f: F) -> bool {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err()
    }

    /// The item's whole claim, as a measurement: after a panic crosses the lock,
    /// `.lock().unwrap()` panics again and the app is dead, while
    /// `lock_recovered` hands the caller a usable guard and says it was poisoned.
    ///
    /// The poison precondition is checked through `Mutex::is_poisoned` — an API
    /// that knows nothing about this file — because without it every leg below
    /// would pass just as well on a mutex that was never poisoned at all.
    #[test]
    fn a_poisoned_mutex_is_answered_rather_than_panicking() {
        let m = Arc::new(Mutex::new(42usize));
        assert!(!m.is_poisoned(), "precondition: a fresh mutex is clean");

        // Leg 1 — the clean path must not report poison it did not see.
        let mut reported = Some(true);
        let mut read = None;
        assert!(
            !did_panic(|| {
                let (guard, poisoned) = lock_recovered(&*m, "gate mutex (clean)");
                reported = Some(poisoned);
                read = Some(*guard);
            }),
            "lock_recovered panicked on a clean mutex"
        );
        assert_eq!(
            reported,
            Some(false),
            "a clean lock was reported as poisoned"
        );
        assert_eq!(read, Some(42), "the clean guard did not carry the value");

        // Precondition — make the poison real, and prove it independently.
        poison_in_a_thread(&m);

        // Leg 2 — the symptom, in the shape it shipped. This panic is the defect:
        // it is what every later command does, forever.
        assert!(
            did_panic(|| {
                let _guard = m.lock().unwrap();
            }),
            "`lock().unwrap()` did NOT panic on a poisoned mutex — the poison \
             precondition is a lie and the recovery leg proves nothing"
        );

        // Leg 3 — the fix, on the same mutex, immediately after.
        let mut reported = Some(false);
        let mut read = None;
        assert!(
            !did_panic(|| {
                let (guard, poisoned) = lock_recovered(&*m, "gate mutex (poisoned)");
                reported = Some(poisoned);
                read = Some(*guard);
            }),
            "lock_recovered panicked on a poisoned mutex — one failed frame still ends the app"
        );
        assert_eq!(
            reported,
            Some(true),
            "the poison was not reported to the caller, so it cannot be shown or acted on"
        );
        assert_eq!(read, Some(42), "recovery handed back an unusable guard");

        // Leg 4 — the refusal is not free: the guard is still a guard, and the
        // value can be written back through it. That is what makes *discard*
        // possible at all; without it the policy could only report, not repair.
        // The guard is dropped before the re-read: `std::sync::Mutex` is not
        // reentrant, and a deadlock here would hang the suite instead of failing.
        {
            let (mut guard, poisoned) = lock_recovered(&*m, "gate mutex (write back)");
            assert!(poisoned, "the poison vanished after one recovery");
            *guard = 99;
        }
        let re_read = *lock_recovered(&*m, "gate mutex (re-read)").0;
        assert_eq!(
            re_read, 99,
            "a recovered guard did not carry the write: recovery is a read-only repair"
        );

        // Leg 5 — and the flag never clears, which is *why* the policies below
        // key on the slot contents and not on this bit. Measured, not asserted
        // from documentation: five acquisitions later the mutex is still poisoned
        // while every one of them was answered.
        for i in 0..5 {
            let (_guard, poisoned) = lock_recovered(&*m, &format!("gate mutex (repeat {i})"));
            assert!(poisoned, "the poison vanished after {i} recoveries");
        }
        assert!(
            m.is_poisoned(),
            "std::sync::Mutex cleared its poison flag — the permanence this policy is built on \
             is gone, and a flag-keyed refusal would now be correct instead of fatal"
        );
    }

    /// `preview_disposition`, arm by arm, on a `usize` standing in for the
    /// renderer. Every slot below is **hand-seeded by this test**, which is exactly
    /// why the test is not enough on its own: it can only say what the policy does
    /// with the slot it is handed, and the shipped caller hands it a *different*
    /// slot each frame because `preview_guard` refills it on every accepted frame.
    /// The session claim — one poison, one refusal — lives in
    /// `one_poison_with_a_renderer_held_refuses_exactly_one_frame`, which drives the
    /// real composition. Read the two together, and do not mistake leg 4 for a
    /// session test: it replays the *arm*, with `None` seated by hand each time.
    #[test]
    fn preview_disposition_discards_and_refuses_arm_by_arm() {
        // Leg 1 — clean: nothing to decide, and nothing touched.
        let mut slot = Some(7usize);
        assert!(
            preview_disposition(false, &mut slot).is_ok(),
            "a clean preview lock was refused"
        );
        assert_eq!(
            slot,
            Some(7),
            "a clean lock discarded a renderer it had no reason to drop"
        );

        // Leg 2 — clean and already empty: the ordinary path right after an
        // export released the device. Not an error, and not a black frame.
        let mut empty: Option<usize> = None;
        assert!(
            preview_disposition(false, &mut empty).is_ok(),
            "an empty slot after an export was treated as a failure"
        );

        // Leg 3 — poisoned, renderer still in the slot: discard, refuse, and say
        // what happened in terms the webview can show.
        let mut slot = Some(7usize);
        let refused = preview_disposition(true, &mut slot);
        let reason = match refused {
            Err(r) => r,
            Ok(()) => panic!("a poisoned lock holding a renderer was NOT refused"),
        };
        assert_eq!(slot, None, "the renderer the panic crossed was kept anyway");
        for needle in ["panic", "not rendered", "rebuilt"] {
            assert!(
                reason.contains(needle),
                "the preview refusal omits {needle:?}, so the UI cannot say what happened: {reason}"
            );
        }

        // Leg 4 — the *arm*, not the session. Four more poisoned frames, each with
        // the slot hand-seeded to `None` exactly as the arm describes, all on a
        // permanently poisoned mutex: none of them may refuse. What the shipped
        // caller does with the slot between frames is outside this test's reach, and
        // is gated by name in the sequence gate above.
        let mut refusals = 1;
        for frame in 1..=4 {
            let mut s: Option<usize> = None;
            if preview_disposition(true, &mut s).is_err() {
                refusals += 1;
            }
            assert_eq!(s, None, "frame {frame} resurrected a discarded renderer");
        }
        assert_eq!(
            refusals, 1,
            "the preview refused {refusals} times across 5 poisoned frames: recovery is now a \
             permanent refusal, which is the same dead app with a different error string"
        );
    }

    /// `export_disposition`, the refusal leg. It is the one answer this item did
    /// NOT copy from the preview path: the export command must not drop the
    /// renderer, because that would decide GPU-device ownership from the wrong
    /// side of the boundary — defect 6's question. So it refuses, leaves the slot
    /// exactly as it found it, and names the action that unblocks it.
    #[test]
    fn an_export_refuses_only_while_a_poisoned_slot_holds_a_renderer() {
        // Legs 1 and 3 — clean, and poisoned-with-nothing-held: both start.
        for (poisoned, holds) in [(false, true), (false, false), (true, false)] {
            assert!(
                export_disposition(poisoned, holds).is_ok(),
                "export was refused for poisoned={poisoned}, slot_holds_renderer={holds} — \
                 only the (poisoned, holds) pair may refuse"
            );
        }

        // Leg 2 — the collision.
        let refused = export_disposition(true, true);
        let reason = match refused {
            Err(r) => r,
            Ok(()) => panic!(
                "an export started while a poisoned lock still held a GPU device — that is the \
                 two-device crash the release step exists to avoid"
            ),
        };
        for needle in ["refused", "device", "preview frame"] {
            assert!(
                reason.contains(needle),
                "the export refusal omits {needle:?}: it does not say what was refused or what \
                 to do about it: {reason}"
            );
        }
    }

    /// The composition gate this item shipped without: ONE poison, then eleven
    /// frames driven through `preview_guard` — the function the shipped preview
    /// commands actually call — so the slot is refilled between frames exactly as
    /// the caller refills it. Everything below is a number, and every number is
    /// asserted as an equality rather than `>= 1`: refusals == 1, refills == 1,
    /// rendered == 10, and the poison flag clear at the end.
    ///
    /// Why the flag is the one that matters: `std::sync::Mutex` never clears it by
    /// itself, and the accepted path puts a renderer back in the slot. So without a
    /// clear on the recovery path the alternation is self-sustaining — frame 1 refuses
    /// and discards, frame 2 is accepted *and builds a new renderer*, frame 3 therefore
    /// meets `(poisoned, Some)` again and refuses — which is defect 5 rewritten as "one
    /// bad frame halves the preview for the rest of the session", with every surviving
    /// frame also paying a fresh `PreviewRenderer::new()` (a whole wgpu device request)
    /// because the slot was emptied two frames earlier.
    ///
    /// Falsifiable, and measured in this exact shape: delete both `preview.clear_poison()`
    /// calls in `preview_guard` and the same 11 frames print 6 refusals and 5 refills with
    /// the flag still set (smoke/I7-1-mut-q1.log; smoke/I7-1-symptom.log is the same shape
    /// measured on the tree before the fix existed) — the PM's standalone probe reproduced
    /// against the shipped caller rather than against a copy of it. Delete only the
    /// *discard* site and these counts do not move (smoke/I7-1-mut-q2.log): the rebuild
    /// site clears the flag one frame later, so that site is pinned by `preview_guard`'s
    /// call-site gate instead. No GPU and no window are needed, which is the whole point of
    /// `preview_guard` being generic: `G` is `usize` and `init` is a counter.
    #[test]
    fn one_poison_with_a_renderer_held_refuses_exactly_one_frame() {
        let frames = 11usize;
        let m = Arc::new(Mutex::new(Some(7usize)));
        poison_in_a_thread(&m);

        let refills = Rc::new(Cell::new(0usize));
        let mut refusals = 0;
        let mut rendered = 0;
        let mut held = None;
        for _frame in 1..=frames {
            let refills = Rc::clone(&refills);
            match preview_guard(&*m, "sequence gate", move || {
                refills.set(refills.get() + 1);
                // A number the poisoned slot could not already be holding: if this
                // value is what survives, it was built *after* the panic.
                Ok(100usize)
            }) {
                Ok(guard) => {
                    rendered += 1;
                    held = *guard;
                }
                Err(reason) => {
                    refusals += 1;
                    assert!(
                        reason.contains("discarded"),
                        "the refusal does not say what happened to the renderer: {reason}"
                    );
                }
            }
        }
        println!(
            "SEQUENCE poison-with-renderer: frames={frames} rendered={rendered} \
             refused={refusals} refills={} poison_at_end={}",
            refills.get(),
            m.is_poisoned()
        );
        assert_eq!(
            refusals, 1,
            "one poison caused {refusals} refusals across {frames} frames: the preview is \
             running at half rate for the rest of the session, which is the dead-app defect \
             with a user-facing error string instead of a panic"
        );
        assert_eq!(
            rendered,
            frames - 1,
            "{rendered} of {frames} frames rendered after a single poison"
        );
        assert_eq!(
            refills.get(),
            1,
            "the caller built {} renderers to survive one poison: every accepted frame is \
             paying a wgpu device request because the slot keeps being emptied again",
            refills.get()
        );
        assert_eq!(
            held,
            Some(100),
            "the renderer on the last frame is {held:?}, not the one recovery built — either \
             the poisoned renderer survived, or the fresh one did not outlive the next frame"
        );
        assert!(
            !m.is_poisoned(),
            "the poison flag survived the recovery, so the next stretch of clean frames is \
             still armed to alternate"
        );
    }

    /// The other half of the licence, and the reason there are two clear sites and
    /// not one. A panic can cross the preview lock while the slot is **empty** — the
    /// ordinary state right after an export released the device — and then no frame
    /// is ever refused, so the discard site is never reached. If the rebuild site
    /// does not clear, a perfectly healthy renderer ends up on a permanently
    /// poisoned mutex and the frame *after* it refuses with nothing wrong to point
    /// at. Asserted as refusals == 0, refills == 1, all frames rendered, flag clear.
    ///
    /// Falsifiable by deleting the `preview.clear_poison()` that follows the `init`
    /// in `preview_guard`: measured 1 refusal and 2 refills over the same 5 frames,
    /// flag clear at the end because the discard site catches it a frame later
    /// (smoke/I7-1-mut-q3.log). Deleting only the discard site leaves this gate green
    /// (smoke/I7-1-mut-q2.log) — the two sites are falsified by different gates, which
    /// is why the counts are printed rather than merely asserted.
    #[test]
    fn one_poison_with_an_empty_slot_refuses_no_frame_at_all() {
        let frames = 5usize;
        let m: Arc<Mutex<Option<usize>>> = Arc::new(Mutex::new(None));
        poison_in_a_thread(&m);

        let refills = Rc::new(Cell::new(0usize));
        let mut refusals = 0;
        let mut rendered = 0;
        let mut held = None;
        for _frame in 1..=frames {
            let refills = Rc::clone(&refills);
            match preview_guard(&*m, "sequence gate (empty start)", move || {
                refills.set(refills.get() + 1);
                Ok(11usize)
            }) {
                Ok(guard) => {
                    rendered += 1;
                    held = *guard;
                }
                Err(_) => refusals += 1,
            }
        }
        println!(
            "SEQUENCE poison-with-empty-slot: frames={frames} rendered={rendered} \
             refused={refusals} refills={} poison_at_end={}",
            refills.get(),
            m.is_poisoned()
        );
        assert_eq!(
            refusals, 0,
            "a poison that never held a renderer refused {refusals} frames: the flag outlived \
             the value it described, and every later frame inherits a refusal with no cause"
        );
        assert_eq!(
            rendered, frames,
            "only {rendered} of {frames} frames rendered"
        );
        assert_eq!(
            refills.get(),
            1,
            "the slot was rebuilt {} times for one poison with an empty start",
            refills.get()
        );
        assert_eq!(held, Some(11), "the surviving renderer is {held:?}");
        assert!(!m.is_poisoned(), "the flag survived the rebuild");
    }

    /// The export side, measured instead of promised: `export_disposition`'s
    /// refusal is clearable by one preview frame, so a once-poisoned session may
    /// refuse exactly **one** export and must let every later one through. The
    /// rounds interleave the two commands the way the UI can reach them — press
    /// export, then let a preview frame run.
    ///
    /// The refill count here is 5 and not 1, and that is this fix working as well as
    /// it can: every accepted export empties the slot to release the device, which
    /// is defect 6's design and lane B3's to answer, so each following frame builds
    /// a renderer again. Pinning the number is the point — a change to that cycle
    /// has to come here and say why, instead of quietly making the preview pay for it.
    ///
    /// Falsifiable by deleting both `preview.clear_poison()` sites: the same 6 rounds
    /// then measure 3 export refusals and only 3 exports started, with 3 preview
    /// refusals and the flag still set (smoke/I7-1-mut-q1.log) — the refusal string
    /// telling the user to render a preview frame while that frame is itself refused
    /// is the loop this gate exists to make impossible.
    #[test]
    fn a_poisoned_session_refuses_one_export_and_then_exports() {
        let rounds = 6usize;
        let m = Arc::new(Mutex::new(Some(7usize)));
        poison_in_a_thread(&m);

        let refills = Rc::new(Cell::new(0usize));
        let mut export_refusals = 0;
        let mut export_starts = 0;
        let mut preview_refusals = 0;
        for round in 0..rounds {
            match export_preview_release(&*m, "sequence gate (export)") {
                Ok(()) => export_starts += 1,
                Err(reason) => {
                    export_refusals += 1;
                    assert!(
                        reason.contains("preview frame"),
                        "round {round}: the export refusal does not name the action that \
                         unblocks it: {reason}"
                    );
                }
            }
            let refills = Rc::clone(&refills);
            if preview_guard(&*m, "sequence gate (preview)", move || {
                refills.set(refills.get() + 1);
                Ok(100usize)
            })
            .is_err()
            {
                preview_refusals += 1;
            }
        }
        println!(
            "SEQUENCE export/preview interleaved: rounds={rounds} export_refusals=\
             {export_refusals} export_starts={export_starts} preview_refusals={preview_refusals} \
             refills={} poison_at_end={}",
            refills.get(),
            m.is_poisoned()
        );
        assert_eq!(
            export_refusals, 1,
            "one poison refused {export_refusals} exports across {rounds} rounds: the refusal \
             string tells the user to render a preview frame, so if it survives one the remedy \
             is a lie"
        );
        assert_eq!(
            export_starts,
            rounds - 1,
            "only {export_starts} exports started"
        );
        assert_eq!(
            preview_refusals, 1,
            "the preview refused {preview_refusals} frames"
        );
        assert_eq!(
            refills.get(),
            5,
            "the preview rebuilt {} times over {rounds} export rounds — that is the \
             export-releases-the-device cycle, so if it moved, the move has to be argued for \
             here and not discovered in a profile",
            refills.get()
        );
        assert!(!m.is_poisoned(), "the session ended still poisoned");
    }

    /// `clear_poison` is the API that turns "a panic crossed this and we do not know
    /// what it left" into "the guarded data is sound", so *where* it appears is a
    /// rule and not a style question. The rule, as code: exactly two calls, both in
    /// `preview_guard`, each one line after the value the panic crossed has stopped
    /// existing (the discard) or been replaced by one this process just built (the
    /// init), and always on the preview mutex through the `preview` binding.
    ///
    /// The `host` and `osc` mutexes are deliberately never cleared. For those the
    /// chosen answer is *continue* precisely because the document cannot be vouched
    /// for, so erasing the flag there would be the "recover without saying so"
    /// mistake pointed the other way — and the flag is the only record a later
    /// reader has that a panic crossed the project state.
    ///
    /// This is the only gate that reddens on *every* deletion probe: all three of
    /// smoke/I7-1-mut-q1/q2/q3.log fail here, and q2 fails **only** here — deleting
    /// the discard site changes no measured count, so a call-site rule is the whole
    /// of what pins it. That is a limit of the design, disclosed rather than argued
    /// away: the licence for that site is the prose above and the fact that the slot
    /// holds `None` on the line before it, not a behaviour anyone can replay headless.
    #[test]
    fn clear_poison_is_called_only_where_the_value_is_gone_or_new() {
        let lines = shell_code_lines(include_str!("lib.rs"));
        let called = lines
            .iter()
            .filter(|l| l.contains("clear_poison("))
            .cloned()
            .collect::<Vec<String>>();
        assert_eq!(
            called,
            vec!["preview.clear_poison();".to_string(); 2],
            "the clear_poison call sites are {called:?}: the licensed two are the discard and \
             the rebuild in preview_guard, and a third has to be argued for in prose before it \
             can be counted here"
        );
        let findings = lines
            .iter()
            .filter(|l| unlicensed_clear_poison(l))
            .cloned()
            .collect::<Vec<String>>();
        assert!(
            findings.is_empty(),
            "poison cleared where this file has not argued that the value is gone or new: \
             {findings:?}"
        );

        // Non-vacuity, the way the lock ban does it: the shapes this gate exists to
        // catch must actually be caught, or the green above means nothing.
        for seed in [
            "state.host.clear_poison();",
            "state.osc.clear_poison();",
            "host.clear_poison();",
            "self.host.lock().unwrap().clear_poison();",
            "m.clear_poison();",
        ] {
            assert!(
                unlicensed_clear_poison(seed),
                "{seed:?} would pass the clear_poison gate, so the gate is not checking what \
                 its name claims"
            );
        }
        assert!(
            !unlicensed_clear_poison("preview.clear_poison();"),
            "the licensed form is itself a finding, which makes this gate a ban on the word"
        );
    }

    /// Any `clear_poison` that is not the one licensed call on the preview mutex.
    /// A different receiver is a finding even when the receiver *is* the preview
    /// mutex under another name: the binding's name is where the argument lives, and
    /// `m.clear_poison()` in a helper is how a third site would arrive without
    /// anyone having to say which value it was certifying.
    fn unlicensed_clear_poison(line: &str) -> bool {
        line.contains("clear_poison(") && line.trim() != "preview.clear_poison();"
    }

    /// `preview_slot`'s two arms, as behaviour — possible because the function is
    /// generic over the slot, so no device is needed to reach the `None` arm the
    /// old code panicked on.
    #[test]
    fn an_empty_preview_slot_answers_with_a_showable_reason() {
        let mut held = Some(3usize);
        {
            let r = preview_slot(&mut held).expect("a held renderer must not be refused");
            *r = 4;
        }
        assert_eq!(
            held,
            Some(4),
            "preview_slot did not hand back the renderer itself"
        );

        let mut empty: Option<usize> = None;
        let reason = match preview_slot(&mut empty) {
            Ok(_) => panic!("an empty slot was reported as a renderer"),
            Err(r) => r,
        };
        assert!(
            !reason.trim().is_empty(),
            "the empty-slot reason is blank — the UI would show a failure with no explanation"
        );
        assert!(
            reason.contains("not rendered"),
            "the empty-slot reason does not say the frame was not rendered: {reason}"
        );
    }

    /// The ban, read off the source: no command path may acquire a lock or a
    /// slot with a form that panics. `include_str!` + comment-filtering is the
    /// pattern this repo already uses in
    /// `capability_report_never_derives_verified_from_a_build_feature`, and it is
    /// the only gate available here — cargo has run `build.rs` before a test
    /// binary exists, and a WebView2 panic mid-drag needs a human.
    #[test]
    fn no_command_unwraps_a_lock_or_a_slot() {
        let lines = shell_code_lines(include_str!("lib.rs"));
        // Anti-vacuity, before the assertion that could pass on an empty scan:
        // the region the ban reads has to be the command bodies. Each of these is a
        // line this lane put there, so if the scanner ever returns nothing — or
        // stops early — the gate fails here instead of quietly greening over an
        // unscanned file. Three of them name what I7 moved the acquisition behind —
        // the composition (`preview_guard` / `export_preview_release`) and the two
        // command paths that call it — because the ban has to keep covering the
        // preview path after the extraction, not just the lines it used to be able
        // to see.
        let joined = lines.join("\n");
        for seen in [
            "let (mut host, _) = lock_recovered(&state.host",
            "let (mut guard, poisoned) = lock_recovered(preview, what);",
            "let mut guard = preview_renderer(&state.preview)?;",
            "if let Err(reason) = export_preview_release(&state.preview",
            "#[tauri::command]",
        ] {
            assert!(
                joined.contains(seen),
                "shell_code_lines does not contain {seen:?}: the ban is scanning the wrong \
                 region and cannot fail."
            );
        }
        let hits = ban_hits_on(lines);
        assert!(
            hits.is_empty(),
            "the shell still has panic-on-poison acquisitions: {hits:?}. Every one of them is \
             the dead-app defect: the first frame that panics inside a lock makes every later \
             command panic too, while the window keeps showing the last image."
        );
    }

    /// Positive controls for the detector, on a fixture built to disagree with
    /// it. A ban that silently matches nothing is the same shape as the bug it
    /// gates, and a ban that reads its own prose is a ban that can never be
    /// re-run safely, so both halves get their own leg:
    ///  * the matcher must report the five shapes that shipped at `63bc321`
    ///    (read out of `git show 63bc321:apps/desktop/src-tauri/src/lib.rs`);
    ///  * the scanner must drop comments and everything inside `mod tests`, so
    ///    the gate's own explanations and the panics these tests manufacture do
    ///    not count as violations.
    #[test]
    fn the_lock_ban_reaches_the_forms_it_bans() {
        // Leg 1 — the five shapes that shipped at 63bc321, one per acquisition
        // site: host, osc, the preview slot, the `if let Ok` that skipped the
        // device release, and the expect form this repo also bans.
        let pre_item_source = [
            "    let mut host = state.host.lock().unwrap();",
            "    let g = state.osc.lock().unwrap();",
            "    let renderer = guard.as_mut().unwrap();",
            "    if let Ok(mut guard) = state.preview.lock() {",
            "    state.host.lock().expect(\"host poisoned\").live_audio_status()",
        ];
        let mut missed: Vec<&str> = Vec::new();
        for line in pre_item_source {
            if lock_ban_hit(line.trim()).is_none() {
                missed.push(line);
            }
        }
        assert!(
            missed.is_empty(),
            "the ban does not match the shapes it exists for: {missed:?}. \
             `no_command_unwraps_a_lock_or_a_slot` is passing vacuously."
        );

        // Leg 2 — the scanner, on a fixture whose three parts must NOT be
        // treated alike: one comment, one command line, one test-module line.
        // Expected: exactly one kept, exactly one hit, and it is the command.
        let fixture = concat!(
            "//! `.lock().unwrap()` in a module doc, which is prose.\n",
            "/// Why not `.as_mut().unwrap()`: explained in a doc comment.\n",
            "    let h = state.host.lock().unwrap();\n",
            "#[cfg(test)]\n",
            "mod tests {\n",
            "    let o = state.osc.lock().unwrap();\n",
            "}\n",
        );
        let kept = shell_code_lines(fixture);
        assert_eq!(
            kept.len(),
            1,
            "shell_code_lines kept {kept:?} of the fixture's 7 lines: comments and everything \
             after `#[cfg(test)]` must be dropped and the command body must not be."
        );
        let hits = ban_hits_on(kept);
        assert_eq!(
            hits.len(),
            1,
            "the ban reported {hits:?} on a fixture holding exactly one violation"
        );
        assert!(
            hits[0].0.contains("state.host"),
            "the ban reported the wrong line: {:?} — it is matching prose or test scaffolding",
            hits[0]
        );
    }

    /// Everything in the shell outside `mod tests`, one trimmed line at a time,
    /// comments dropped. Exempting the test module is not a loophole: these gates
    /// *manufacture* panics, and the ban is about the shipped command paths.
    /// Pure — the anti-vacuity for what it must contain lives in the gate that
    /// uses it, so a fixture can be scanned without tripping that gate.
    fn shell_code_lines(shell: &str) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        for line in shell.lines() {
            let t = line.trim();
            if t == "#[cfg(test)]" {
                break;
            }
            if t.is_empty() || t.starts_with("//") {
                continue;
            }
            lines.push(t.to_string());
        }
        lines
    }

    /// One forbidden acquisition, or none.
    fn lock_ban_hit(line: &str) -> Option<&'static str> {
        const FORBIDDEN: [&str; 4] = [
            ".lock().unwrap()",
            ".lock().expect(",
            ".as_mut().unwrap()",
            ".as_ref().unwrap()",
        ];
        for f in FORBIDDEN {
            if line.contains(f) {
                return Some(f);
            }
        }
        // The fourth shape is not an unwrap and was still a bug: `if let Ok(guard)`
        // on a poisoned lock skips the arm, so the release step never happened and
        // the export started anyway, green. A silent skip is the same defect as a
        // panic, just quieter.
        if line.contains("if let Ok(") && line.contains(".lock()") {
            return Some("if let Ok(..) = ..lock()");
        }
        None
    }

    /// Report every line that breaks the ban, with its text rather than just a
    /// count, so a failure names the line to fix.
    fn ban_hits_on(lines: Vec<String>) -> Vec<(String, &'static str)> {
        lines
            .iter()
            .filter_map(|l| lock_ban_hit(l).map(|f| (l.clone(), f)))
            .collect()
    }

    /// What this block cannot gate, recorded as a defect rather than as a pass:
    ///  * a real WebView2 panic inside a held lock, mid-drag, and whether the
    ///    window shows the refusal — that is a human at the app;
    ///  * that `render_graph` actually leaves a torn `roots_cache` behind when it
    ///    panics. `preview_disposition` assumes the worst about a poisoned GPU
    ///    path; proving the worst requires a panic injected inside
    ///    `crates/app/src/export.rs`, which is out of scope for lane B1;
    ///  * that `PreviewRenderer::new()` succeeds on this machine's GPU after a
    ///    discard, i.e. that the rebuild the refusal promises really happens. The
    ///    arm is gated as a ban on the panic form instead, and the rebuild path is
    ///    exercised by the GUI, not by these tests.
    #[test]
    fn the_unreachability_claims_are_stated_not_assumed() {
        // Nothing to assert beyond the doc above, and a test with no assertion
        // would be padding. What IS asserted: each policy has exactly one caller
        // in the shipped shell, so no third path inherits a refusal rule nobody
        // gated it against. Definition lines are excluded by the `fn ` prefix —
        // without that the census would be counting the rules, not the callers.
        let lines = shell_code_lines(include_str!("lib.rs"));
        let callers_of = |policy: &str| {
            lines
                .iter()
                .filter(|l| l.contains(&format!("{policy}(")) && !l.starts_with("fn "))
                .cloned()
                .collect::<Vec<String>>()
        };
        for policy in ["preview_disposition", "export_disposition"] {
            let callers = callers_of(policy);
            assert_eq!(
                callers.len(),
                1,
                "{policy} is called by {callers:?}: expected exactly one caller in the shell. \
                 A second one would be a new path reusing a refusal rule it has not been gated \
                 against."
            );
        }
    }
}
