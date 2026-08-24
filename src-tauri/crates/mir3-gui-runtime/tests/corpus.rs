use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use mir3_gui_runtime::{
    execute_request, DataProfileSnapshot, DeviceKind, RuntimeOperation, RuntimeRequest,
    RuntimeServer, StartRequest, Viewport, PROTOCOL_VERSION,
};

#[test]
fn official_guilayout_corpus_never_panics() {
    let Some(root) = corpus_root() else {
        eprintln!("跳过 996 GUI Runtime 语料测试：未设置 MIR3_GUI_CORPUS");
        return;
    };
    let modules = collect_modules(&root);
    let layouts = modules
        .iter()
        .filter(|(path, _)| path.starts_with("GUILayout/") && path.ends_with(".lua"))
        .collect::<Vec<_>>();
    let runnable = layouts
        .iter()
        .filter(|(_, source)| has_scene_entry(source))
        .collect::<Vec<_>>();
    assert!(
        layouts.len() >= 180,
        "GUILayout 语料数量异常：{}",
        layouts.len()
    );
    assert!(
        runnable.len() >= 160,
        "可运行场景数量异常：{}",
        runnable.len()
    );

    let mut runtime_success = 0usize;
    let mut static_fallback = 0usize;
    for (index, (layout_path, _)) in runnable.iter().enumerate() {
        let mut server = RuntimeServer::new();
        let response = execute_request(
            &mut server,
            RuntimeRequest {
                protocol_version: PROTOCOL_VERSION,
                request_id: format!("corpus-{index}"),
                operation: RuntimeOperation::Start(StartRequest {
                    scene_id: format!("project:{}", layout_path.trim_end_matches(".lua")),
                    layout_path: (*layout_path).clone(),
                    modules: modules.clone(),
                    device: DeviceKind::Mobile,
                    viewport: Viewport::default(),
                    data_profile: DataProfileSnapshot::default(),
                    limits: None,
                }),
            },
        );
        if response.ok {
            runtime_success += 1;
        } else {
            // 主界面会保留 last-valid scene，并转到静态 Scene Composition。
            static_fallback += 1;
        }
    }
    assert_eq!(runtime_success + static_fallback, runnable.len());
    eprintln!(
        "996 GUI Runtime corpus: layouts={}, runnable={}, runtime={}, static_fallback={}",
        layouts.len(),
        runnable.len(),
        runtime_success,
        static_fallback
    );
}

fn corpus_root() -> Option<PathBuf> {
    std::env::var_os("MIR3_GUI_CORPUS")
        .map(PathBuf::from)
        .filter(|path| path.join("GUILayout").is_dir())
}

fn collect_modules(root: &Path) -> BTreeMap<String, String> {
    let mut modules = BTreeMap::new();
    for directory in ["GUILayout", "GUIExport", "GUIData"] {
        collect_modules_inner(root, &root.join(directory), &mut modules);
    }
    modules
}

fn collect_modules_inner(root: &Path, directory: &Path, modules: &mut BTreeMap<String, String>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_modules_inner(root, &path, modules);
            continue;
        }
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lua"))
        {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let Ok(source) = fs::read(&path) else {
            continue;
        };
        modules.insert(
            relative.to_string_lossy().replace('\\', "/"),
            String::from_utf8_lossy(&source).into_owned(),
        );
    }
}

fn has_scene_entry(source: &str) -> bool {
    source.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("function ") && line.contains(".main(")
    })
}
