use mir3_domain::{DomainStore, SafeXlsCellUpdate, SafeXlsDraftPatch};
use std::{fs, path::PathBuf};

#[test]
fn installed_project_workbooks_open_patch_save_and_restore_on_disposable_copies() {
    let Some(source) = std::env::var_os("MIR3_WORKBOOK_CORPUS_ROOT") else {
        return;
    };
    let base = std::env::temp_dir().join(format!(
        "mir3-workbook-real-{}-{}",
        std::process::id(),
        mir3_domain::now_millis()
    ));
    let project = base.join("project");
    fs::create_dir_all(project.join("客户端/dev")).unwrap();
    fs::create_dir_all(project.join("引擎/Mir200/Envir/Data")).unwrap();
    fs::write(project.join("引擎/version.txt"), "1.8").unwrap();
    let names = ["cfg_AntiRobotQuestion.xls", "cfg_equip.xls", "cfg_item.xls"];
    for name in names {
        let path = format!("引擎/Mir200/Envir/Data/{name}");
        fs::copy(PathBuf::from(&source).join(&path), project.join(&path)).unwrap();
    }
    let store = DomainStore::new(base.join("data")).unwrap();
    let imported = store.import_project(&project).unwrap();
    store.scan_project(&imported.id, || false).unwrap();
    for (system, expected) in [("equipment", "cfg_equip.xls"), ("item", "cfg_item.xls")] {
        let files = store
            .query_domain_files(
                &imported.id,
                system,
                &mir3_domain::DomainFileQuery {
                    text: String::new(),
                    limit: Some(100),
                    offset: None,
                },
            )
            .unwrap();
        assert!(
            files.iter().any(|file| file.path.ends_with(expected)),
            "{system} did not expose {expected}"
        );
        assert!(!files
            .iter()
            .any(|file| file.path.ends_with("cfg_AntiRobotQuestion.xls")));
    }
    for name in names {
        let path = format!("引擎/Mir200/Envir/Data/{name}");
        let original = fs::read(project.join(&path)).unwrap();
        let opened = store.workbench_xls_open(&imported.id, &path).unwrap();
        let copy = opened.working_copy.unwrap();
        let sheet = &opened.data.sheets[0];
        let old = sheet.rows[0][0].clone();
        let result = store
            .domain_working_xls_patch(
                &imported.id,
                &copy.id,
                SafeXlsDraftPatch {
                    relative_path: path.clone(),
                    draft_id: copy.id.clone(),
                    expected_revision: copy.revision,
                    expected_sha256: opened.data.workbook.sha256,
                    updates: vec![SafeXlsCellUpdate {
                        sheet: sheet.sheet.clone(),
                        row: 0,
                        column: 0,
                        expected_value: Some(old),
                        value: serde_json::json!("MIR3 临时副本测试"),
                    }],
                },
            )
            .unwrap();
        assert_eq!(fs::read(project.join(&path)).unwrap(), original);
        let saved = store
            .save_domain_working_copy(&imported.id, &copy.id, result.revision, true)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        store
            .restore_domain_save_node(&imported.id, &saved.save_node.id)
            .unwrap();
        assert_eq!(fs::read(project.join(&path)).unwrap(), original);
    }
    fs::remove_dir_all(base).unwrap();
}
