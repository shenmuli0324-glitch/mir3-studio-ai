use mir3_ui::{parse_document, CompatibilityStatus, Mir3UiNodeType};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// 官方语料不进入仓库；设置 MIR3_GUI_CORPUS 后运行只读回归。
#[test]
fn parses_optional_official_corpus_without_panicking() {
    let Ok(root) = std::env::var("MIR3_GUI_CORPUS") else {
        return;
    };
    let files = lua_files(Path::new(&root));
    assert!(
        !files.is_empty(),
        "MIR3_GUI_CORPUS did not contain Lua files"
    );
    let mut nodes = 0usize;
    let mut diagnostics = 0usize;
    let mut unknown = 0usize;
    for path in &files {
        let bytes = fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let source = String::from_utf8_lossy(&bytes);
        let relative = path.strip_prefix(&root).unwrap_or(path).to_string_lossy();
        let document = parse_document(&source, &relative, "corpus", "utf-8", "\n")
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        serde_json::to_vec(&document)
            .unwrap_or_else(|error| panic!("{} serialization: {error}", path.display()));
        let node_by_id: HashMap<_, _> = document
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let mut linked_children = HashSet::new();
        for node in &document.nodes {
            if let Some(parent_id) = &node.parent_id {
                let parent = node_by_id.get(parent_id.as_str()).unwrap_or_else(|| {
                    panic!("{} missing parent node {}", path.display(), parent_id)
                });
                assert!(
                    parent.children.contains(&node.id),
                    "{} parent {} does not link child {}",
                    path.display(),
                    parent_id,
                    node.id
                );
                assert!(
                    linked_children.insert(node.id.as_str()),
                    "{} child {} is linked more than once",
                    path.display(),
                    node.id
                );
            } else {
                assert!(
                    document.roots.contains(&node.id),
                    "{} root list is missing {}",
                    path.display(),
                    node.id
                );
            }
            for asset in node.asset_slots.values() {
                if asset.value.trim().is_empty() {
                    continue;
                }
                assert!(
                    document
                        .assets
                        .iter()
                        .any(|entry| entry.logical_path == asset.value),
                    "{} missing asset index entry for {}",
                    path.display(),
                    asset.value
                );
            }
            for span in node.source_binding.property_spans.values() {
                assert!(
                    span.start_byte <= span.end_byte && span.end_byte <= source.len(),
                    "{} property span is outside source",
                    path.display()
                );
            }
        }
        nodes += document.nodes.len();
        diagnostics += document.diagnostics.len();
        unknown += document
            .nodes
            .iter()
            .filter(|node| {
                node.node_type == Mir3UiNodeType::Unsupported
                    || node.compatibility.status == CompatibilityStatus::Unknown
            })
            .count();
    }
    eprintln!(
        "MIR3 GUI corpus: files={}, nodes={}, diagnostics={}, unknown={}",
        files.len(),
        nodes,
        diagnostics,
        unknown,
    );
    if files.len() == 249 {
        assert_eq!(nodes, 5_948);
    }
    assert_eq!(unknown, 0, "registered official widget types must be known");
}

fn lua_files(root: &Path) -> Vec<PathBuf> {
    let mut output = Vec::new();
    visit(root, &mut output);
    output.sort();
    output
}

fn visit(path: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "lua") {
            output.push(path);
        }
    }
}
