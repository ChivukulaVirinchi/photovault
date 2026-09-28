//! ONNX Runtime initialization and management

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use ort::execution_providers::ExecutionProvider;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;

/// Latch: only log the provider-probe result the first time a session is
/// built. Subsequent sessions reuse whichever provider won without spamming.
static EP_LOGGED: AtomicBool = AtomicBool::new(false);

/// Successfully registered session providers. This is explicitly NOT a
/// claim about GPU node placement; different models can use different providers.
static ACTIVE_PROVIDER: Mutex<&'static str> = Mutex::new("Not initialized");
#[cfg(target_os = "windows")]
static WINDOWS_RUNTIME_LIBS: OnceLock<Vec<libloading::Library>> = OnceLock::new();
static ORT_INITIALIZED: OnceLock<()> = OnceLock::new();
static ORT_INIT_LOCK: Mutex<()> = Mutex::new(());

/// User-facing label for the active execution provider, e.g. "DirectML",
/// "CPU", or a qualified GPU-registration label. Uninitialized until a session succeeds.
pub fn active_execution_provider() -> &'static str {
    *ACTIVE_PROVIDER.lock().unwrap_or_else(|e| e.into_inner())
}

/// Platform-specific ONNX Runtime library name
#[cfg(target_os = "windows")]
const ORT_LIB_NAME: &str = "onnxruntime.dll";
#[cfg(target_os = "macos")]
const ORT_LIB_NAME: &str = "libonnxruntime.dylib";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
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
    fn usable_runtime_file(path: &Path) -> bool {
        std::fs::metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() >= 1024 * 1024)
    }

    fn runtime_file_name_matches(name: &str) -> bool {
        #[cfg(target_os = "macos")]
        {
            name.starts_with("libonnxruntime") && name.ends_with(".dylib")
        }
        #[cfg(not(target_os = "macos"))]
        {
            name.starts_with(ORT_LIB_NAME)
        }
    }

    fn find_runtime_in_dir(dir: &Path) -> Option<PathBuf> {
        let direct = dir.join(ORT_LIB_NAME);
        if Self::usable_runtime_file(&direct) {
            return Some(direct);
        }

        // Also check for versioned variants (e.g. libonnxruntime.so.1.23.0)
        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if Self::runtime_file_name_matches(name) && Self::usable_runtime_file(&path) {
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

        // 1b. Check installed optional asset-pack roots. The asset pack
        // stores runtimes under platform subdirectories; bootstrap owns
        // that layout so health checks and dynamic loading agree.
        if let Some(candidate) = crate::bootstrap::onnx_runtime_path() {
            tracing::info!(
                "Using ONNX Runtime from optional asset-pack path: {}",
                candidate.display()
            );
            return Some(candidate);
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
        if ORT_INITIALIZED.get().is_some() {
            return Ok(Self);
        }

        let _guard = ORT_INIT_LOCK.lock().map_err(|_| {
            ort::Error::new("ONNX Runtime initialization lock is poisoned".to_string())
        })?;
        if ORT_INITIALIZED.get().is_some() {
            return Ok(Self);
        }

        if let Some(dylib_path) = Self::resolve_dylib_path() {
            #[cfg(target_os = "windows")]
            if WINDOWS_RUNTIME_LIBS.get().is_none() {
                use libloading::os::windows::{
                    Library, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
                };
                let root = dylib_path
                    .parent()
                    .ok_or_else(|| ort::Error::new("Runtime path has no parent"))?;
                let mut libraries = Vec::new();
                for dependency in [
                    "DirectML.dll",
                    "onnxruntime_providers_shared.dll",
                    ORT_LIB_NAME,
                ] {
                    let path = root.join(dependency);
                    if !path.is_file() {
                        continue;
                    } // Standard CPU installations remain supported.
                      // SAFETY: load only runtime assets beside the selected ORT DLL.
                      // Keep handles alive for all sessions; dependency search is restricted
                      // to that directory and Windows' safe default directories.
                    let library = unsafe {
                        Library::load_with_flags(
                            &path,
                            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
                        )
                    }
                    .map_err(|e| {
                        ort::Error::new(format!("Failed loading {}: {e}", path.display()))
                    })?;
                    libraries.push(library.into());
                }
                let _ = WINDOWS_RUNTIME_LIBS.set(libraries);
            }
            ort::init_from(&dylib_path)?.commit();
            let _ = ORT_INITIALIZED.set(());
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
        // Registration failure must be observable: try complete sessions one at
        // a time, then retry CPU if a GPU cannot initialize this particular model.
        let path = path.as_ref();
        let providers = vec![
            #[cfg(target_os = "windows")]
            (
                "DirectML registered (placement unverified)",
                ort::execution_providers::DirectMLExecutionProvider::default().build(),
            ),
            #[cfg(target_os = "linux")]
            (
                "CUDA registered (placement unverified)",
                ort::execution_providers::CUDAExecutionProvider::default().build(),
            ),
            #[cfg(target_os = "linux")]
            (
                "OpenVINO registered (placement unverified)",
                ort::execution_providers::OpenVINO::default()
                    .with_device_type("GPU")
                    .build(),
            ),
            #[cfg(target_os = "macos")]
            (
                "CoreML registered (placement unverified)",
                ort::execution_providers::CoreMLExecutionProvider::default().build(),
            ),
            (
                "CPU",
                ort::execution_providers::CPUExecutionProvider::default().build(),
            ),
        ];

        if !EP_LOGGED.swap(true, Ordering::Relaxed) {
            Self::probe_and_log_providers();
        }
        let mut last_error = None;
        for (label, provider) in providers {
            let attempt = (|| {
                let builder = Session::builder()?
                    .with_optimization_level(GraphOptimizationLevel::Level3)?
                    .with_intra_threads(intra_threads.clamp(1, 2))?
                    .with_parallel_execution(false)?
                    .with_config_entry("session.intra_op.allow_spinning", "0")?
                    .with_config_entry("session.inter_op.allow_spinning", "0")?;
                let builder = if label.starts_with("DirectML") {
                    builder.with_memory_pattern(false)?
                } else {
                    builder
                };
                builder
                    .with_execution_providers([provider.error_on_failure()])?
                    .commit_from_file(path)
            })();
            match attempt {
                Ok(session) => {
                    let mut active = ACTIVE_PROVIDER.lock().unwrap_or_else(|e| e.into_inner());
                    *active = if *active == "Not initialized" || *active == label {
                        label
                    } else {
                        "Mixed model providers (see logs; placement unverified)"
                    };
                    tracing::info!(model = %path.display(), provider = label, "Inference session initialized");
                    return Ok(session);
                }
                Err(error) => {
                    tracing::warn!(model = %path.display(), provider = label, %error,
                        "Provider initialization failed; trying fallback");
                    last_error = Some(error);
                }
            }
        }
        Err(last_error.expect("CPU provider is always attempted"))
    }

    /// A one-way CPU fallback after an inference-time provider/device failure.
    pub fn load_cpu_model(path: &Path) -> ort::Result<Session> {
        let session = Self::load_cpu_model_with_threads(path, 1)?;
        *ACTIVE_PROVIDER.lock().unwrap_or_else(|e| e.into_inner()) =
            "CPU fallback used (see per-model logs)";
        tracing::warn!(model = %path.display(), "Inference moved to CPU after device/provider failure");
        Ok(session)
    }

    /// Build a CPU-only session with an explicit share of the aggregate face
    /// processing budget. Face workers use this path because constructing one
    /// DirectML session per worker caused severe cold-start and throughput
    /// regressions on integrated GPUs. Other ML features may still use the
    /// provider-selecting path above.
    pub fn load_cpu_model_with_threads(path: &Path, intra_threads: usize) -> ort::Result<Session> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(intra_threads.max(1))?
            .with_parallel_execution(false)?
            .with_config_entry("session.intra_op.allow_spinning", "0")?
            .with_config_entry("session.inter_op.allow_spinning", "0")?
            .with_execution_providers([ort::execution_providers::CPUExecutionProvider::default()
                .build()
                .error_on_failure()])?
            .commit_from_file(path)?;
        let mut active = ACTIVE_PROVIDER.lock().unwrap_or_else(|e| e.into_inner());
        *active = if *active == "Not initialized" || *active == "CPU" {
            "CPU"
        } else {
            "Mixed model providers (see logs; placement unverified)"
        };
        tracing::info!(model = %path.display(), threads = intra_threads.max(1), "CPU inference session initialized");
        Ok(session)
    }

    /// Load a latency-sensitive CPU model without the expensive offline-style
    /// graph rewrites used by long-running batch inference. This is the right
    /// tradeoff for semantic text search: the session must become available
    /// quickly, while each interaction runs only one tiny text input.
    pub fn load_cpu_model_for_interactive(path: &Path) -> ort::Result<Session> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level1)?
            .with_intra_threads(1)?
            .with_parallel_execution(false)?
            .with_config_entry("session.intra_op.allow_spinning", "0")?
            .with_config_entry("session.inter_op.allow_spinning", "0")?
            .with_execution_providers([ort::execution_providers::CPUExecutionProvider::default()
                .build()
                .error_on_failure()])?
            .commit_from_file(path)?;
        tracing::info!(model = %path.display(), "Interactive CPU inference session initialized");
        Ok(session)
    }

    /// One-shot probe: log which execution providers are usable on this
    /// machine, and say plainly which of them this build will actually use.
    ///
    /// Two things this deliberately does NOT claim:
    ///
    /// 1. That face detection/embedding will use a GPU. It will not —
    ///    `FaceDetector`/`FaceEmbedder` both call
    ///    `load_cpu_model_with_threads`, because building one DirectML session
    ///    per worker caused severe cold-start and throughput regressions on
    ///    integrated GPUs. Only the provider-selecting path (semantic search)
    ///    still probes for a GPU, so the log says so instead of implying faces
    ///    are GPU-bound.
    fn probe_and_log_providers() {
        let mut accelerated: Vec<&'static str> = Vec::new();

        #[cfg(target_os = "windows")]
        {
            let ep = ort::execution_providers::DirectMLExecutionProvider::default();
            if ep.is_available().unwrap_or(false) {
                accelerated.push("DirectML");
            }
        }

        #[cfg(target_os = "linux")]
        {
            let ep = ort::execution_providers::CUDAExecutionProvider::default();
            if ep.is_available().unwrap_or(false) {
                accelerated.push("CUDA");
            }

            let ep = ort::execution_providers::OpenVINO::default();
            if ep.is_available().unwrap_or(false) {
                accelerated.push("OpenVINO");
            }
        }

        #[cfg(target_os = "macos")]
        {
            let ep = ort::execution_providers::CoreMLExecutionProvider::default();
            if ep.is_available().unwrap_or(false) {
                accelerated.push("CoreML");
            }
        }

        tracing::info!(
            accelerated = %if accelerated.is_empty() { "none".to_string() } else { accelerated.join(", ") },
            "Execution providers present. Face detection/embedding always run on CPU; \
             the provider-selecting path (semantic search) tries accelerated providers first, \
             then CPU."
        );
    }
}
