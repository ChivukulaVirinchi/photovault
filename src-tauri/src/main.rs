#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    install_tracing();
    install_panic_log();
    smriti_tauri_lib::run()
}

/// Wire up a tracing subscriber so `tracing::info!` / `warn!` / `error!`
/// from anywhere in the workspace actually shows up. Without this, every
/// log macro is a no-op — which is what made the geocoding diagnostics
/// invisible during debugging.
///
/// Default filter: `info`. Override via `RUST_LOG=smriti=debug,…` in the
/// environment. Output goes to stderr so it doesn't compete with stdout
/// (which Tauri uses for IPC framing in some configurations).
fn install_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,hyper=warn,reqwest=warn"));
    let _ = fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(true)
        .try_init();
}

/// Write panic messages to a crash log file before the process aborts.
///
/// Background: the release profile has `panic = "abort"` and the binary
/// runs under `windows_subsystem = "windows"` (no attached console), so
/// any panic — including those in tokio `spawn_blocking` workers — was
/// previously invisible. The geonames backfill and similar long jobs
/// could die silently and leave users wondering why the app "restarted
/// for no reason." Panic hooks run before the abort, so this captures
/// the failure even with `panic = abort` configured.
///
/// The log lives next to the app data so we don't litter the user's
/// home directory; if that's unavailable we fall back to `crash.log`
/// in the CWD as a last resort.
fn install_panic_log() {
    std::panic::set_hook(Box::new(|info| {
        use std::io::Write;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown location>".to_string());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("<no message>");
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string();
        let line = format!(
            "[{now}] panic on thread '{thread}' at {location}: {payload}\n"
        );

        // Echo to stderr in case devtools or a debugger is attached.
        let _ = std::io::stderr().write_all(line.as_bytes());

        if let Some(path) = crash_log_path() {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = f.write_all(line.as_bytes());
                let _ = f.flush();
            }
        }
    }));
}

fn crash_log_path() -> Option<std::path::PathBuf> {
    // Try a stable user-scoped location first. dirs is already a
    // transitive dep via Tauri, but we avoid adding it here and use
    // env vars to stay zero-dep.
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("LOCALAPPDATA") {
            let dir = std::path::PathBuf::from(appdata).join("Smriti");
            let _ = std::fs::create_dir_all(&dir);
            return Some(dir.join("crash.log"));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(home) = std::env::var("HOME") {
            let dir = std::path::PathBuf::from(home).join(".local/share/smriti");
            let _ = std::fs::create_dir_all(&dir);
            return Some(dir.join("crash.log"));
        }
    }
    Some(std::path::PathBuf::from("crash.log"))
}
