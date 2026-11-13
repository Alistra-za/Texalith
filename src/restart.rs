//! A lost wgpu device can't be recovered inside the process: eframe can't recreate it and egui-wgpu panics on the
//! next frame ("Failed to create staging buffer"). So, as Tessalith does (`gui/src/crash_log.rs:76-90,133-135`), the
//! app notices it first and starts itself again. `render3d` losing its own D3D11 device is handled in `preview.rs`.
use eframe::egui_wgpu::RenderState;
use std::ffi::OsString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Marks a process as a restart, so the window can say why and the loop guard holds.
pub const FLAG: &str = "--after-device-loss";

static LOST: AtomicBool = AtomicBool::new(false);
static REASON: Mutex<String> = Mutex::new(String::new());
static STARTED: OnceLock<Instant> = OnceLock::new();

/// Call once, first thing in `main`.
pub fn mark_started() {
    let _ = STARTED.set(Instant::now());
}

pub fn is_restart() -> bool {
    std::env::args().any(|a| a == FLAG)
}

/// wgpu reports a lost device to no one unless asked.
pub fn watch(rs: &RenderState) {
    rs.device.set_device_lost_callback(|reason, message| {
        if let Ok(mut r) = REASON.lock() {
            *r = format!("{reason:?}: {message}");
        }
        LOST.store(true, Ordering::Release);
    });
}

/// Check at the start of every frame: this frame's paint would panic.
pub fn lost() -> bool {
    LOST.load(Ordering::Acquire)
}

pub fn reason() -> String {
    REASON.lock().map(|r| r.clone()).unwrap_or_default()
}

/// A restarted process that loses the device again within 60 s stays down, so a broken driver can't loop forever.
pub fn should_restart(is_restart: bool, uptime: Duration) -> bool {
    !is_restart || uptime >= Duration::from_secs(60)
}

/// The second net. A loss that hits during a frame's own paint panics before the next frame's [`lost`] check can run,
/// and the callback may not have fired yet, so the panic's own text counts too.
pub fn restart_on_lost_panic(args: Vec<OsString>) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        let text = info.payload_as_str().unwrap_or_default();
        if lost() || text.contains("Failed to create staging buffer") {
            restart_and_exit(&args);
        }
    }));
}

/// Says why the app is about to close: a release build has no console, so without this the window just vanishes.
/// Blocks until the box is closed, so no test may reach it.
pub fn fatal(text: &str) {
    eprintln!("{text}");
    rfd::MessageDialog::new().set_level(rfd::MessageLevel::Error).set_title("Texalith").set_description(text).show();
}

/// shortcut: a save running at this moment is cut off (each file is whole, a set of maps may be half new); wait for
/// `save::wait` here if that is ever seen. It must not wait without a limit: this also runs inside a panic.
pub fn restart_and_exit(args: &[OsString]) -> ! {
    eprintln!("Texalith: graphics device lost ({})", reason());
    let uptime = STARTED.get().map(Instant::elapsed).unwrap_or_default();
    if should_restart(is_restart(), uptime) {
        let spawned = std::env::current_exe().and_then(|exe| std::process::Command::new(exe).args(args).arg(FLAG).spawn());
        if let Err(e) = spawned {
            fatal(&format!("Texalith lost the graphics device and could not start itself again: {e}"));
            std::process::exit(1);
        }
        std::process::exit(0);
    }
    fatal("Texalith lost the graphics device twice within a minute, so it will close. Update the graphics driver, then start Texalith again.");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::should_restart;
    use std::time::Duration;

    #[test]
    fn a_first_loss_restarts_and_a_quick_second_one_does_not() {
        assert!(should_restart(false, Duration::ZERO));
        assert!(!should_restart(true, Duration::from_secs(59)));
        assert!(should_restart(true, Duration::from_secs(60)));
    }
}
