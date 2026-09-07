use mir3_domain::{now_millis, DomainStore};
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn production_validation_requires_schema_evidence_for_engine_generalization() {
    let base = fixture_root("missing-schema-evidence");
    let project_root = base.join("project");
    create_project_layout(&project_root);
    fs::write(
        project_root.join("客户端/dev/Level/level.txt"),
        "not a projected level record\n",
    )
    .unwrap();

    let store = DomainStore::new(base.join("data")).unwrap();
    let project = store.import_project(&project_root).unwrap();
    store.scan_project(&project.id, || false).unwrap();
    let report = store.validate_domain_system(&project.id, "level").unwrap();

    assert!(!report.valid);
    assert!(report
        .diagnostics
        .contains(&"DOMAIN_ENGINE_EVIDENCE_MISSING:resource-schema-validation".to_string()));
    fs::remove_dir_all(base).ok();
}

#[test]
fn production_validation_does_not_invent_client_engine_projection_from_names() {
    let base = fixture_root("client-engine-match-key");
    let project_root = base.join("project");
    create_project_layout(&project_root);
    fs::write(
        project_root.join("客户端/dev/Level/client-level.txt"),
        "level=1\nrequiredExperience=100\n",
    )
    .unwrap();
    fs::write(
        project_root.join("引擎/Mir200/Level/engine-level.txt"),
        "level=1\nrequiredExperience=200\n",
    )
    .unwrap();

    let store = DomainStore::new(base.join("data")).unwrap();
    let project = store.import_project(&project_root).unwrap();
    store.scan_project(&project.id, || false).unwrap();
    let report = store.validate_domain_system(&project.id, "level").unwrap();
    assert_eq!(report.owned_files, 0);
    assert!(!report
        .validators
        .iter()
        .any(|validator| validator.kind == "client-engine-consistency"));
    fs::remove_dir_all(base).ok();
}

fn create_project_layout(root: &Path) {
    fs::create_dir_all(root.join("客户端/dev/Level")).unwrap();
    fs::create_dir_all(root.join("引擎/Mir200/Level")).unwrap();
    fs::write(root.join("引擎/version.txt"), "1.2.0\n").unwrap();
}

fn fixture_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "mir3-runtime-validator-{name}-{}-{}",
        std::process::id(),
        now_millis()
    ))
}
