//! Ableton Link adapter — thin FFI over the official `abl_link` C API
//! (`https://github.com/Ableton/link`, GPL-2.0-or-later). Only compiled with the
//! `link` feature. See `implementation/UPSTREAM.md` and DECISIONS D010/D011.
//!
//! Transport is kept separate from the internal/MIDI transport. Offline rendering
//! never depends on Link; it can be enabled/disabled at runtime.
//!
//! NOTE (licensing): Link is GPLv2+. Locally building and verifying the adapter is
//! fine; distributing a Flux binary that links Link makes it a derivative work and
//! must be GPL-compatible, or Link must ship as a separately loaded plugin.

#![cfg(feature = "link")]

use std::ffi::c_void;

type AblLink = *mut c_void;
type SessionState = *mut c_void;

#[link(name = "abl_link")]
extern "C" {
    fn abl_link_create(bpm: f64) -> AblLink;
    fn abl_link_destroy(link: AblLink);
    fn abl_link_is_enabled(link: AblLink) -> bool;
    fn abl_link_enable(link: AblLink, enable: bool);
    fn abl_link_is_start_stop_sync_enabled(link: AblLink) -> bool;
    fn abl_link_enable_start_stop_sync(link: AblLink, enabled: bool);
    fn abl_link_num_peers(link: AblLink) -> u64;
    fn abl_link_clock_micros(link: AblLink) -> i64;

    fn abl_link_create_session_state() -> SessionState;
    fn abl_link_destroy_session_state(state: SessionState);
    fn abl_link_capture_app_session_state(link: AblLink, state: SessionState);
    fn abl_link_commit_app_session_state(link: AblLink, state: SessionState);

    fn abl_link_tempo(state: SessionState) -> f64;
    fn abl_link_set_tempo(state: SessionState, bpm: f64);
    fn abl_link_beat_at_time(state: SessionState, time_micros: i64, quantum: f64) -> f64;
    fn abl_link_phase_at_time(state: SessionState, time_micros: i64, quantum: f64) -> f64;
    fn abl_link_set_is_playing(state: SessionState, playing: bool, time_micros: i64);
    fn abl_link_is_playing(state: SessionState) -> bool;
}

/// A Link session: a peer that shares tempo/beat/transport on the local network.
pub struct LinkSession {
    link: AblLink,
}

/// Snapshot of shared transport state at a given host time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkState {
    pub tempo: f64,
    pub beat: f64,
    pub phase: f64,
    pub playing: bool,
    pub num_peers: u64,
}

impl LinkSession {
    /// Create and enable a Link session at `bpm` with start/stop sync.
    pub fn new(bpm: f64) -> Option<Self> {
        unsafe {
            let link = abl_link_create(bpm);
            if link.is_null() {
                return None;
            }
            abl_link_enable(link, true);
            abl_link_enable_start_stop_sync(link, true);
            Some(Self { link })
        }
    }

    pub fn is_enabled(&self) -> bool {
        unsafe { abl_link_is_enabled(self.link) }
    }

    pub fn start_stop_sync_enabled(&self) -> bool {
        unsafe { abl_link_is_start_stop_sync_enabled(self.link) }
    }

    pub fn num_peers(&self) -> u64 {
        unsafe { abl_link_num_peers(self.link) }
    }

    /// Host clock in microseconds (Link's monotonic clock).
    pub fn clock_micros(&self) -> i64 {
        unsafe { abl_link_clock_micros(self.link) }
    }

    /// Capture the shared state at the current host time.
    pub fn state(&self) -> LinkState {
        unsafe {
            let ss = abl_link_create_session_state();
            if ss.is_null() {
                return LinkState {
                    tempo: 120.0,
                    beat: 0.0,
                    phase: 0.0,
                    playing: false,
                    num_peers: 0,
                };
            }
            abl_link_capture_app_session_state(self.link, ss);
            let t = abl_link_clock_micros(self.link);
            let state = LinkState {
                tempo: abl_link_tempo(ss),
                beat: abl_link_beat_at_time(ss, t, 4.0),
                phase: abl_link_phase_at_time(ss, t, 4.0),
                playing: abl_link_is_playing(ss),
                num_peers: abl_link_num_peers(self.link),
            };
            abl_link_destroy_session_state(ss);
            state
        }
    }

    /// Propose a new tempo to the session (peers adopt per Link rules).
    pub fn set_tempo(&self, bpm: f64) {
        unsafe {
            let ss = abl_link_create_session_state();
            if ss.is_null() {
                return;
            }
            abl_link_capture_app_session_state(self.link, ss);
            abl_link_set_tempo(ss, bpm);
            abl_link_commit_app_session_state(self.link, ss);
            abl_link_destroy_session_state(ss);
        }
    }

    /// Start or stop the shared transport at the current host time.
    pub fn set_playing(&self, playing: bool) {
        unsafe {
            let ss = abl_link_create_session_state();
            if ss.is_null() {
                return;
            }
            abl_link_capture_app_session_state(self.link, ss);
            let now = abl_link_clock_micros(self.link);
            abl_link_set_is_playing(ss, playing, now);
            abl_link_commit_app_session_state(self.link, ss);
            abl_link_destroy_session_state(ss);
        }
    }
}

impl Drop for LinkSession {
    fn drop(&mut self) {
        if !self.link.is_null() {
            unsafe { abl_link_destroy(self.link) };
            self.link = std::ptr::null_mut();
        }
    }
}

// FFI handles are not Send by default; the session is owned by one thread in Flux.
unsafe impl Send for LinkSession {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn session_creates_enabled_with_start_stop() {
        let s = LinkSession::new(120.0).expect("link session");
        assert!(s.is_enabled());
        assert!(s.start_stop_sync_enabled());
        // A lone session has no peers.
        assert_eq!(s.num_peers(), 0);
        let st = s.state();
        assert!((st.tempo - 120.0).abs() < 1e-6, "tempo {}", st.tempo);
        assert!(!st.playing, "not playing until started");
    }

    #[test]
    fn clock_advances() {
        let s = LinkSession::new(120.0).unwrap();
        let t0 = s.clock_micros();
        std::thread::sleep(Duration::from_millis(20));
        let t1 = s.clock_micros();
        assert!(t1 > t0, "clock advanced {t0} -> {t1}");
    }

    #[test]
    fn set_playing_reflected_in_state() {
        let s = LinkSession::new(120.0).unwrap();
        s.set_playing(true);
        assert!(s.state().playing, "playing after set_playing(true)");
        s.set_playing(false);
        assert!(!s.state().playing);
    }

    #[test]
    fn two_sessions_discover_and_share_tempo() {
        // Two independent Link instances on this host should discover each
        // other over the loopback interface and converge on tempo.
        let a = LinkSession::new(120.0).unwrap();
        let b = LinkSession::new(90.0).unwrap();

        // Propose a new tempo from A and give discovery a moment.
        a.set_tempo(133.0);

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut a_peers = 0;
        let mut b_peers = 0;
        while Instant::now() < deadline {
            a_peers = a.num_peers();
            b_peers = b.num_peers();
            if a_peers >= 1 && b_peers >= 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(a_peers >= 1, "A should see >=1 peer, saw {a_peers}");
        assert!(b_peers >= 1, "B should see >=1 peer, saw {b_peers}");

        // Tempo should have converged across the session.
        let ta = a.state().tempo;
        let tb = b.state().tempo;
        assert!((ta - tb).abs() < 1.0, "tempo converged A={ta} B={tb}");
    }

    fn wait_peers(a: &LinkSession, b: &LinkSession, secs: u64) -> bool {
        let deadline = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < deadline {
            if a.num_peers() >= 1 && b.num_peers() >= 1 {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }

    #[test]
    fn single_master_tempo_wins_across_peers() {
        // One session changes tempo; the peer that was left alone adopts it —
        // i.e. a single proposer's tempo becomes the session tempo.
        let a = LinkSession::new(120.0).unwrap();
        let b = LinkSession::new(120.0).unwrap();
        assert!(wait_peers(&a, &b, 5), "peers must connect");

        a.set_tempo(101.0);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut ta = 0.0;
        let mut tb = 0.0;
        while Instant::now() < deadline {
            ta = a.state().tempo;
            tb = b.state().tempo;
            if (ta - 101.0).abs() < 1.0 && (tb - 101.0).abs() < 1.0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(
            (ta - 101.0).abs() < 1.5,
            "A adopts proposed tempo, got {ta}"
        );
        assert!(
            (tb - 101.0).abs() < 1.5,
            "B adopts proposed tempo, got {tb}"
        );
    }

    #[test]
    fn start_stop_propagates_to_peer() {
        let a = LinkSession::new(120.0).unwrap();
        let b = LinkSession::new(120.0).unwrap();
        assert!(wait_peers(&a, &b, 5), "peers must connect");

        a.set_playing(true);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut b_playing = false;
        while Instant::now() < deadline {
            b_playing = b.state().playing;
            if b_playing {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(b_playing, "start propagated to peer");
        assert!(a.state().playing, "initiator is playing");

        a.set_playing(false);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stopped = false;
        while Instant::now() < deadline {
            if !b.state().playing {
                stopped = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(stopped, "stop propagated to peer");
    }
}
