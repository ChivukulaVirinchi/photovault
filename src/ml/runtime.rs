//! ONNX Runtime initialization and management

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use ort::execution_providers::ExecutionProvider;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;

/// Latch: only log the provider-probe result the first time a session is
/// built. Subsequent sessions reuse whichever provider won without spamming.
static EP_LOGGED: AtomicBool = AtomicBool::new(false);

/// Best-known label for the actual execution provider in use, populated
/// the first time a session is built. Read by Settings to surface
/// "Face inference: GPU (DirectML)" or "Face inference: CPU".
///
/// Best-effort: ORT silently falls back across providers per-node, so
/// this reflects the *best* provider that probed available, not a
/// proof that every op runs on it.
static ACTIVE_PROVIDER: OnceLock<&'static str> = OnceLock::new();

/// User-facing label for the active execution provider, e.g. "DirectML",
/// "CUDA", "CoreML", "CPU". Returns "CPU" before any session is built.
pub fn active_execution_provider() -> &'static str {
    ACTIVE_PROVIDER.get().copied().unwrap_or("CPU")
}

/// Platform-specific ONNX Runtime library name
#[cfg(target_os = "windows")]
const ORT_LIB_NAME: &str = "onnxruntime.dll";
#[cfg(not(target_os = "windows"))]
const ORT_LIB_NAME: &str = "libonnxruntime.so";

/// ONNX Runtime environment wrapper
///
/// Manages the global ONNX Runtime environment and provides
/// helper methods for loading models.
///
/// Uses `load-dynamic` feature: the runtime library is loaded at runtime.
/// The library is resolved in this order:
/// 1. `ORT_DYLIB_PATH` environment variable (if set)
/// 2. `libs/onnxruntime/<LIB>` relative to the executable
/// 3. `libs/onnxruntime/<LIB>` relative to the current working directory
pub struct OnnxRuntime;

impl OnnxRuntime {
    fn find_runtime_in_dir(dir: &Path) -> Option<PathBuf> {
        let direct = dir.join(ORT_LIB_NAME);
        if direct.exists() {
            return Some(direct);
        }

        // Also check for versioned variants (e.g. libonnxruntime.so.1.23.0)
        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with(ORT_LIB_NAME) {
                    return Some(path);
                }
            }
        }

        None
    }

    /// Resolve the path to the ONNX Runtime shared library.
    ///
    /// Checks ORT_DYLIB_PATH env var first, then looks in libs/onnxruntime/
    /// relative to the executable and current working directory.
    fn resolve_dylib_path() -> Option<PathBuf> {
        // 1. Check ORT_DYLIB_PATH environment variable
        if let Ok(path) = std::env::var("ORT_DYLIB_PATH") {
            let p = PathBuf::from(&path);
            if p.exists() {
                tracing::info!("Using ONNX Runtime from ORT_DYLIB_PATH: {}", p.display());
                return Some(p);
            }
            tracing::warn!("ORT_DYLIB_PATH set but file not found: {}", path);
        }

        // 1b. Check installed optional asset-pack roots
        let roots = [
            crate::bootstrap::default_asset_install_dir(),
            std::env::var("PHOTOVAULT_ASSET_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("")),
        ];
        for root in roots {
            if root.as_os_str().is_empty() {
                continue;
            }
            let candidate_dir = root.join("libs").join("onnxruntime");
            if let Some(candidate) = Self::find_runtime_in_dir(&candidate_dir) {
                tracing::info!(
                    "Using ONNX Runtime from optional asset-pack path: {}",
                    candidate.display()
                );
                return Some(candidate);
            }
        }

        let rel_dir = Path::new("libs").join("onnxruntime");

        // 2. Relative to the executable. Also tries two levels up
        // because `cargo tauri dev` runs the binary from
        // `target/debug/`, and the dev-tree `libs/onnxruntime/` lives
        // at the workspace root (target/debug/../.. = workspace).
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                for base in [exe_dir.to_path_buf(), exe_dir.join("..").join("..")] {
                    let candidate_dir = base.join(&rel_dir);
                    if let Some(candidate) = Self::find_runtime_in_dir(&candidate_dir) {
                        tracing::info!(
                            "Using ONNX Runtime from exe-relative path: {}",
                            candidate.display()
                        );
                        return Some(candidate);
                    }
                }
            }
        }

        // 3. Relative to the current working directory. Also walks one
        // level up because `cargo tauri dev` sets CWD to src-tauri/,
        // not the workspace root.
        if let Ok(cwd) = std::env::current_dir() {
            let mut bases: Vec<PathBuf> = vec![cwd.clone()];
            if let Some(parent) = cwd.parent() {
                bases.push(parent.to_path_buf());
            }
            for base in bases {
                let candidate_dir = base.join(&rel_dir);
                if let Some(candidate) = Self::find_runtime_in_dir(&candidate_dir) {
                    tracing::info!(
                        "Using ONNX Runtime from cwd-relative path: {}",
                        candidate.display()
                    );
                    return Some(candidate);
                }
            }
        }

        None
    }

    /// Initialize the ONNX Runtime global environment.
    ///
    /// Dynamically loads libonnxruntime.so at runtime. The library is searched
    /// in the order described on [`OnnxRuntime`].
    ///
    /// This should be called once at application startup, before creating any sessions.
    pub fn init() -> ort::Result<Self> {
        if let Some(dylib_path) = Self::resolve_dylib_path() {
            ort::init_from(&dylib_path)?.commit();
            tracing::info!(
                "ONNX Runtime initialized (dynamic) from: {}",
                dylib_path.display()
            );
        } else {
            return Err(ort::Error::new(
                format!(
                    "ONNX Runtime library not found. Set ORT_DYLIB_PATH or place {} (1.23.x) in libs/onnxruntime/",
                    ORT_LIB_NAME
                ),
            ));
        }
        Ok(Self)
    }

    /// Load an ONNX model with a specific number of intra-op threads.
    ///
    /// Attempts to register GPU execution providers in platform-specific
    /// priority order, then falls back to CPU if none initialize. The CPU
    /// provider is always appended last so session creation never fails due
    /// to missing GPU drivers or runtime libraries.
    ///
    /// Platform priority:
    ///   Windows: DirectML  (covers NVIDIA/AMD/Intel/Qualcomm via D3D12)
    ///   Linux:   CUDA      (NVIDIA). ROCm could be added via a cargo flag later.
    ///   macOS:   CoreML    (Apple Silicon + AMD on Intel Macs)
    ///
    /// `intra_threads` applies to the CPU provider; GPU providers ignore it.
    pub fn load_model_with_threads<P: AsRef<Path>>(
        &self,
        path: P,
        intra_threads: usize,
    ) -> ort::Result<Session> {
        // Build the priority list. Every entry is a Dispatch value that ORT
        // will try to initialize; if the underlying native library or driver
        // is unavailable, ORT silently skips it and continues down the list.
        let mut providers: Vec<ort::execution_providers::ExecutionProviderDispatch> = Vec::new();

        #[cfg(target_os = "windows")]
        {
            providers.push(ort::execution_providers::DirectMLExecutionProvider::default().build());
        }

        #[cfg(target_os = "linux")]
        {
            providers.push(ort::execution_providers::CUDAExecutionProvider::default().build());
        }

        #[cfg(target_os = "macos")]
        {
            providers.push(ort::execution_providers::CoreMLExecutionProvider::default().build());
        }

        providers.push(ort::execution_providers::CPUExecutionProvider::default().build());

        if !EP_LOGGED.swap(true, Ordering::Relaxed) {
            Self::probe_and_log_providers();
        }

        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(intra_threads)?
            .with_execution_providers(providers)?
            .commit_from_file(path)?;

        Ok(session)
    }

    /// One-shot probe: log which execution providers appear usable on this
    /// machine. Best-effort — actual provider binding happens per-session.
    fn probe_and_log_providers() {
        let mut available: Vec<&'static str> = Vec::new();

        #[cfg(target_os = "windows")]
        {
            let ep = ort::execution_providers::DirectMLExecutionProvider::default();
            if ep.is_available().unwrap_or(false) {
                available.push("DirectML");
            }
        }

        #[cfg(target_os = "linux")]
        {
            let ep = ort::execution_providers::CUDAExecutionProvider::default();
            if ep.is_available().unwrap_or(false) {
                available.push("CUDA");
            }
        }

        #[cfg(target_os = "macos")]
        {
            let ep = ort::execution_providers::CoreMLExecutionProvider::default();
            if ep.is_available().unwrap_or(false) {
                available.push("CoreML");
            }
        }

        available.push("CPU");

        // Capture the best (front-of-list) provider so Settings can
        // show what's actually engaging — falling back to CPU when no
        // GPU EP probed available.
        let chosen = *available.first().unwrap_or(&"CPU");
        let _ = ACTIVE_PROVIDER.set(chosen);

        if available.len() > 1 {
            tracing::info!(
                "Face inference will try execution providers in order: {}",
                available.join(" -> ")
            );
        } else {
            tracing::info!("Face inference will run on CPU (no GPU providers available)");
        }
    }
}

impl Default for OnnxRuntime {
    fn default() -> Self {
        Self::init().expect("Failed to initialize ONNX Runtime")
    }
}
