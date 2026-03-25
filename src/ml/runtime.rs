//! ONNX Runtime initialization and management

use std::path::{Path, PathBuf};

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;

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

        let rel_dir = Path::new("libs").join("onnxruntime");

        // 2. Relative to the executable
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let candidate_dir = exe_dir.join(&rel_dir);
                if let Some(candidate) = Self::find_runtime_in_dir(&candidate_dir) {
                    tracing::info!(
                        "Using ONNX Runtime from exe-relative path: {}",
                        candidate.display()
                    );
                    return Some(candidate);
                }
            }
        }

        // 3. Relative to the current working directory
        if let Ok(cwd) = std::env::current_dir() {
            let candidate_dir = cwd.join(&rel_dir);
            if let Some(candidate) = Self::find_runtime_in_dir(&candidate_dir) {
                tracing::info!(
                    "Using ONNX Runtime from cwd-relative path: {}",
                    candidate.display()
                );
                return Some(candidate);
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

    /// Load an ONNX model from a file path into a Session.
    pub fn load_model<P: AsRef<Path>>(&self, path: P) -> ort::Result<Session> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(path)?;

        Ok(session)
    }
}

impl Default for OnnxRuntime {
    fn default() -> Self {
        Self::init().expect("Failed to initialize ONNX Runtime")
    }
}
