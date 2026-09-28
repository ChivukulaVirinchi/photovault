/// Explicit opt-in: runs native vendor code from ORT_DYLIB_PATH, never downloads.
#[test]
#[ignore = "requires a prepared native runtime; run explicitly with ORT_DYLIB_PATH"]
fn native_runtime_dependency_smoke() {
    assert!(std::env::var_os("ORT_DYLIB_PATH").is_some());
    smriti::ml::runtime::OnnxRuntime::init().expect("native runtime and dependencies load");
    #[cfg(target_os = "windows")]
    {
        use ort::execution_providers::ExecutionProvider;
        assert!(
            ort::execution_providers::DirectMLExecutionProvider::default()
                .is_available()
                .expect("provider availability query")
        );
    }
}

/// Uses synthetic tensors only; no library/photos are opened or modified.
#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires installed face models and a prepared DirectML runtime"]
fn native_face_models_gpu_smoke() {
    use ort::session::Session;
    use ort::value::TensorRef;
    assert!(std::env::var_os("ORT_DYLIB_PATH").is_some());
    smriti::ml::runtime::OnnxRuntime::init().unwrap();
    let profiles = tempfile::tempdir().unwrap();
    for (name, size) in [
        ("scrfd_10g_bnkps.onnx", 640usize),
        ("adaface_ir101_webface12m.onnx", 112),
    ] {
        let path = smriti::bootstrap::model_dir().join(name);
        let started = std::time::Instant::now();
        let mut session = Session::builder()
            .unwrap()
            .with_intra_threads(1)
            .unwrap()
            .with_parallel_execution(false)
            .unwrap()
            .with_memory_pattern(false)
            .unwrap()
            .with_config_entry("session.intra_op.allow_spinning", "0")
            .unwrap()
            .with_profiling(profiles.path().join(name))
            .unwrap()
            .with_execution_providers([
                ort::execution_providers::DirectMLExecutionProvider::default()
                    .build()
                    .error_on_failure(),
            ])
            .unwrap()
            .commit_from_file(path)
            .unwrap();
        let input = vec![0.0f32; 3 * size * size];
        let tensor =
            TensorRef::from_array_view(([1usize, 3, size, size], input.as_slice())).unwrap();
        {
            let output = session.run(ort::inputs![tensor]).unwrap();
            assert!(output.iter().next().is_some());
        }
        let profile = session.end_profiling().unwrap();
        let events: serde_json::Value =
            serde_json::from_slice(&std::fs::read(profile).unwrap()).unwrap();
        let providers: std::collections::BTreeSet<_> = events
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|event| event.get("args")?.get("provider")?.as_str())
            .collect();
        println!("{name}: {:?}; providers={providers:?}", started.elapsed());
        assert!(
            providers
                .iter()
                .any(|provider| provider.to_lowercase().contains("dml")),
            "Expected actual DirectML node execution, not availability alone"
        );
    }
}
