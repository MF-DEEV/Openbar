// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The timer's clock (spec 04 §4.1). The Mac anchors on ContinuousClock,
//! which counts through sleep; Windows' interrupt time does too (not the
//! unbiased one, which skips it). The page measures its own clock against it,
//! and asks for a wake-up at a countdown's deadline, which a hidden webview's
//! throttled timers could otherwise delay by up to a minute.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// The longest a wake-up sleeps before reading the clock again, so a
/// deadline passed during a system sleep is noticed within a second of waking.
const CHECK: f64 = 1000.0;

static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Milliseconds of interrupt time: monotonic, counting sleep.
pub fn now_ms() -> f64 {
    #[cfg(windows)]
    {
        // 100 ns units since boot.
        unsafe {
            windows::Win32::System::WindowsProgramming::QueryInterruptTime() as f64 / 10_000.0
        }
    }
    // CLOCK_BOOTTIME keeps counting while the PC sleeps, as Windows' interrupt time does.
    #[cfg(target_os = "linux")]
    {
        let mut now = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `clock_gettime` fills the struct it is given.
        unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut now) };
        now.tv_sec as f64 * 1000.0 + now.tv_nsec as f64 / 1_000_000.0
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        use std::sync::OnceLock;
        use std::time::Instant;
        static START: OnceLock<Instant> = OnceLock::new();
        START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
    }
}

/// How long to sleep before looking again, or None once `left` ms have run out.
fn next_sleep(left: f64) -> Option<Duration> {
    (left > 0.0).then(|| Duration::from_millis(left.min(CHECK).ceil() as u64))
}

#[tauri::command]
pub fn timer_now_ms() -> f64 {
    now_ms()
}

/// Emits `timer://due` once `delay_ms` of interrupt time has passed. Each
/// call replaces the last; None only cancels.
#[tauri::command]
pub fn timer_wake(app: AppHandle, delay_ms: Option<f64>) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let Some(delay) = delay_ms.filter(|d| d.is_finite()) else {
        return;
    };
    let target = now_ms() + delay.max(0.0);
    std::thread::spawn(move || {
        loop {
            if GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
            match next_sleep(target - now_ms()) {
                Some(wait) => std::thread::sleep(wait),
                None => {
                    let _ = app.emit_to(crate::island::LABEL, "timer://due", ());
                    return;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_never_runs_backwards() {
        let a = now_ms();
        std::thread::sleep(Duration::from_millis(5));
        assert!(now_ms() >= a);
    }

    #[test]
    fn a_wake_up_looks_again_at_least_every_second() {
        assert_eq!(next_sleep(0.0), None);
        assert_eq!(next_sleep(-50.0), None);
        assert_eq!(next_sleep(250.4), Some(Duration::from_millis(251)));
        assert_eq!(next_sleep(90_000.0), Some(Duration::from_secs(1)));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_clock {
    #[test]
    fn the_clock_counts_time_since_boot_and_never_goes_back() {
        // Time since boot, sleep included, as /proc/uptime has it.
        let up = std::fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|u| u.split_whitespace().next()?.parse::<f64>().ok())
            .map(|s| s * 1000.0)
            .unwrap();
        let first = super::now_ms();
        assert!((first - up).abs() < 2000.0, "{first} against uptime {up}");
        std::thread::sleep(std::time::Duration::from_millis(60));
        let second = super::now_ms();
        assert!(
            second >= first + 50.0 && second < first + 1000.0,
            "{first} then {second}"
        );
    }
}
