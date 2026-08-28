//! 预装与已安装插件的增删改查、管理。
//!
//! 包括首次启动的预装插件引导（安装/取消/跳过/待办检测/打开仓库）、已安装
//! 插件的列表/升级/卸载，以及运行期异常的记录与「卸除此插件并继续检测」修复。

use crate::config;
use crate::service::plugin;
use tauri::Emitter;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

/// 获取预装插件列表（含已安装检测结果），首次启动引导界面渲染用
#[tauri::command]
pub async fn get_preinstall_plugins(
    app_handle: AppHandle,
) -> Result<Vec<plugin::PreinstallPlugin>, String> {
    Ok(plugin::list(&app_handle))
}

/// 安装选中的预装插件（`dsh plugin --profile web add <ids...>`），
/// 进程输出实时通过 `preinstall-log` 事件推送；成功后标记引导完成并记录预设指纹。
#[tauri::command]
pub async fn install_preinstall_plugins(
    app_handle: AppHandle,
    ids: Vec<String>,
) -> Result<(), String> {
    plugin::install(&app_handle, &ids).await?;
    let preset_hash = plugin::current_preset_hash(&app_handle);
    config::update_setting(&app_handle, |setting| {
        setting.preinstall_done = true;
        if let Some(hash) = preset_hash {
            setting.preset_hash = Some(hash);
        }
    });
    Ok(())
}

/// 取消正在进行的预装插件安装（网络抖动/限流卡住时用户点“取消”）。
#[tauri::command]
pub async fn cancel_preinstall_plugins(app_handle: AppHandle) {
    plugin::cancel(&app_handle).await;
}

/// 跳过预装插件引导：记录状态与预设指纹，之后不再弹出（除非清单内容变更）
#[tauri::command]
pub async fn skip_preinstall_plugins(app_handle: AppHandle) -> Result<(), String> {
    let preset_hash = plugin::current_preset_hash(&app_handle);
    config::update_setting(&app_handle, |setting| {
        setting.preinstall_done = true;
        if let Some(hash) = preset_hash {
            setting.preset_hash = Some(hash);
        }
    });
    Ok(())
}

/// 是否有新的预装插件需要引导：预设清单内容与上次记录不一致（或老用户无基线）。
/// 资源文件每次安装都被强制覆盖不可比对，只能比对 app-data 里记录的内容指纹。
#[tauri::command]
pub fn get_preinstall_pending(app_handle: AppHandle) -> Result<bool, String> {
    Ok(plugin::preinstall_pending(&app_handle))
}

/// 在系统浏览器中打开预装插件的仓库地址（仅允许预装清单内的 id）
#[tauri::command]
pub async fn open_preinstall_repo(app_handle: AppHandle, id: String) -> Result<(), String> {
    let url = plugin::repo_url_of(&app_handle, &id)
        .ok_or_else(|| format!("PREINSTALL_INVALID_ID: {id}"))?;
    app_handle
        .opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// 当前 profile 已安装插件列表（含解析后的元信息），`use-dsh-plugins` 首次加载用；
/// 之后 Rust 侧监控插件文件，变化时通过 `dsh-plugins-updated` 事件实时推送。
#[tauri::command]
pub fn get_dsh_plugins(app_handle: AppHandle) -> Vec<plugin::DshPlugin> {
    plugin::watch::list(&app_handle)
}

#[tauri::command]
pub fn domain_pack_list(
    app_handle: AppHandle,
) -> Result<Vec<plugin::system::DomainPackStateView>, String> {
    let root = config::get_dsh_data_path(&app_handle).join("domain-packs");
    plugin::system::list_domain_pack_states(&root)
}

#[tauri::command]
pub fn domain_pack_state(
    app_handle: AppHandle,
    system_id: String,
) -> Result<plugin::system::DomainPackStateView, String> {
    let root = config::get_dsh_data_path(&app_handle).join("domain-packs");
    plugin::system::domain_pack_state(&root, &system_id)
}

/// 查询正式签名索引；未注入发布配置时保持本地候选可用并明确返回未配置。
#[tauri::command]
pub async fn domain_pack_update_check(
    app_handle: AppHandle,
    system_id: Option<String>,
) -> Result<plugin::domain_update::DomainPackUpdateCheck, String> {
    let root = config::get_dsh_data_path(&app_handle).join("domain-packs");
    plugin::domain_update::check(&root, system_id.as_deref()).await
}

/// 验签、校验并暂存一个远程候选，不执行激活。
#[tauri::command]
pub async fn domain_pack_update_stage(
    app_handle: AppHandle,
    system_id: String,
    version: String,
) -> Result<plugin::system::DomainPackStateView, String> {
    let root = config::get_dsh_data_path(&app_handle).join("domain-packs");
    plugin::domain_update::stage(&root, &system_id, &version).await
}

#[tauri::command]
pub fn domain_pack_activate(
    app_handle: AppHandle,
    system_id: String,
    expected_candidate_version: String,
    expected_candidate_hash: String,
    confirmed: bool,
) -> Result<plugin::system::DomainPackStateView, String> {
    if !confirmed {
        return Err(
            "DOMAIN_PACK_ACTIVATION_CONFIRMATION_REQUIRED: review candidate before activation"
                .to_string(),
        );
    }
    let root = config::get_dsh_data_path(&app_handle).join("domain-packs");
    let project_service = app_handle.state::<crate::service::project::ProjectService>();
    let activation = plugin::system::activate_domain_pack_with_governance_canary(
        &root,
        &system_id,
        &expected_candidate_version,
        &expected_candidate_hash,
        |activated, transition| {
            let from_version = activated
                .previous
                .as_ref()
                .map(|release| release.version.as_str())
                .ok_or_else(|| format!("DOMAIN_PACK_CURRENT_MISSING: {system_id}"))?;
            let expected = activated
                .current
                .as_ref()
                .map(|release| release.version.as_str())
                .ok_or_else(|| format!("DOMAIN_PACK_CURRENT_MISSING: {system_id}"))?;
            assert_runtime_domain_version(&app_handle, &system_id, expected)?;
            let governance_snapshot = project_service
                .store()
                .snapshot_domain_governance(&system_id)?;
            transition.persist_governance_snapshot(&governance_snapshot)?;
            let migration = project_service.store().migrate_domain_governance(
                &system_id,
                from_version,
                expected,
            )?;
            if !migration.compatible {
                return Err(format!(
                    "DOMAIN_GOVERNANCE_MIGRATION_BLOCKED: {}",
                    migration.conflicts.join(" | ")
                ));
            }
            transition.mark_governance_migrated()?;
            Ok(())
        },
        |value| restore_persisted_governance(project_service.store(), value),
    );
    activation?;
    plugin::system::domain_pack_state(&root, &system_id)
}

#[tauri::command]
pub fn domain_pack_rollback(
    app_handle: AppHandle,
    system_id: String,
    confirmed: bool,
) -> Result<plugin::system::DomainPackStateView, String> {
    if !confirmed {
        return Err(
            "DOMAIN_PACK_ROLLBACK_CONFIRMATION_REQUIRED: confirm before rollback".to_string(),
        );
    }
    let root = config::get_dsh_data_path(&app_handle).join("domain-packs");
    let project_service = app_handle.state::<crate::service::project::ProjectService>();
    let rollback = plugin::system::rollback_domain_pack_with_governance_canary(
        &root,
        &system_id,
        |rolled_back, transition| {
            let from_version = rolled_back
                .previous
                .as_ref()
                .map(|release| release.version.as_str())
                .ok_or_else(|| format!("DOMAIN_PACK_CURRENT_MISSING: {system_id}"))?;
            let expected = rolled_back
                .current
                .as_ref()
                .map(|release| release.version.as_str())
                .ok_or_else(|| format!("DOMAIN_PACK_CURRENT_MISSING: {system_id}"))?;
            assert_runtime_domain_version(&app_handle, &system_id, expected)?;
            let governance_snapshot = project_service
                .store()
                .snapshot_domain_governance(&system_id)?;
            transition.persist_governance_snapshot(&governance_snapshot)?;
            let migration = project_service.store().migrate_domain_governance(
                &system_id,
                from_version,
                expected,
            )?;
            if !migration.compatible {
                return Err(format!(
                    "DOMAIN_GOVERNANCE_MIGRATION_BLOCKED: {}",
                    migration.conflicts.join(" | ")
                ));
            }
            transition.mark_governance_migrated()?;
            Ok(())
        },
        |value| restore_persisted_governance(project_service.store(), value),
    );
    rollback?;
    plugin::system::domain_pack_state(&root, &system_id)
}

fn restore_persisted_governance(
    store: &mir3_domain::DomainStore,
    value: &serde_json::Value,
) -> Result<(), String> {
    let snapshot: mir3_domain::GovernanceSnapshot = serde_json::from_value(value.clone())
        .map_err(|error| format!("DOMAIN_PACK_GOVERNANCE_SNAPSHOT_INVALID: {error}"))?;
    store.restore_domain_governance_snapshot(&snapshot)
}

/// 指针事务失败后必须运行治理补偿；补偿失败与原错误同时返回，禁止伪装成已恢复。
#[cfg(test)]
fn finish_governed_domain_pack_transition<T>(
    transition: Result<T, String>,
    compensate: impl FnOnce() -> Result<(), String>,
) -> Result<T, String> {
    match transition {
        Ok(value) => Ok(value),
        Err(error) => match compensate() {
            Ok(()) => Err(error),
            Err(restore) => Err(format!("{error}; governance_restore={restore}")),
        },
    }
}

#[tauri::command]
pub fn domain_pack_set_enabled(
    app_handle: AppHandle,
    system_id: String,
    enabled: bool,
    confirmed: bool,
) -> Result<plugin::system::DomainPackStateView, String> {
    if !confirmed {
        return Err(
            "DOMAIN_PACK_ENABLE_CONFIRMATION_REQUIRED: confirm before changing package state"
                .to_string(),
        );
    }
    let root = config::get_dsh_data_path(&app_handle).join("domain-packs");
    plugin::system::set_domain_pack_enabled(&root, &system_id, enabled)?;
    if enabled {
        let state = plugin::system::domain_pack_state(&root, &system_id)?;
        let expected = state
            .state
            .current
            .as_ref()
            .map(|release| release.version.as_str())
            .ok_or_else(|| format!("DOMAIN_PACK_CURRENT_MISSING: {system_id}"))?;
        assert_runtime_domain_version(&app_handle, &system_id, expected)?;
    }
    plugin::system::domain_pack_state(&root, &system_id)
}

fn assert_runtime_domain_version(
    app_handle: &AppHandle,
    system_id: &str,
    expected_version: &str,
) -> Result<(), String> {
    let project_service = app_handle.state::<crate::service::project::ProjectService>();
    let active = project_service
        .store()
        .domain_manifest_at_version(system_id, expected_version)?;
    if active.version != expected_version {
        return Err(format!(
            "DOMAIN_PACK_RUNTIME_VERSION_MISMATCH: expected {expected_version}, got {}",
            active.version
        ));
    }
    let project = project_service
        .store()
        .active_project()?
        .ok_or_else(|| "DOMAIN_PACK_ENGINE_PROJECT_REQUIRED: no active project".to_string())?;
    project_service
        .store()
        .assert_project_engine_compatible(&project.id, &active)?;
    Ok(())
}

/// 升级单个已安装插件：`dsh plugin --profile <当前档案> update <id>`，
/// 进程输出通过 `preinstall-log` 事件实时推送。
#[tauri::command]
pub async fn update_dsh_plugin(app_handle: AppHandle, id: String) -> Result<(), String> {
    if plugin::system::is_system_plugin(&id) {
        return Err("PLUGIN_SYSTEM_MANAGED: MIR3 Core Plugin is managed by Studio".to_string());
    }
    plugin::update(&app_handle, &id).await?;
    plugin::watch::force_emit(&app_handle);
    Ok(())
}

/// 卸载单个已安装插件：`dsh plugin --profile <当前档案> remove <id>`，
/// 进程输出通过 `preinstall-log` 事件实时推送。
#[tauri::command]
pub async fn remove_dsh_plugin(app_handle: AppHandle, id: String) -> Result<(), String> {
    if plugin::system::is_system_plugin(&id) {
        return Err("PLUGIN_SYSTEM_MANAGED: MIR3 Core Plugin cannot be removed".to_string());
    }
    plugin::remove(&app_handle, &id).await?;
    plugin::watch::force_emit(&app_handle);
    Ok(())
}

/// 上报插件运行期异常（内嵌页面 / dsh-tauri 桥调用），记录后立即推送新列表，
/// 并推送 `plugin-recovery-required` 让前端弹出「卸除此插件并继续检测」修复界面。
#[tauri::command]
pub fn report_plugin_error(
    app_handle: AppHandle,
    id: String,
    error: String,
    action: Option<String>,
) -> Result<(), String> {
    plugin::errors::record(
        &app_handle,
        &id,
        action.as_deref().unwrap_or("runtime"),
        &error,
    )?;
    plugin::watch::force_emit(&app_handle);
    // 运行期异常：直接推送修复界面（应用仍在运行，前端以醒目对话框呈现）。
    let info = plugin::PluginRecoveryInfo {
        plugins: vec![id],
        reason: "runtime".to_string(),
        detail: String::new(),
        raw_error: error,
    };
    let _ = app_handle.emit(plugin::recovery::RECOVERY_REQUIRED_EVENT, &info);
    Ok(())
}

/// 从启动日志定位导致启动失败的问题插件（含归属到配置根插件）。
///
/// 前端在启动失败时已读过服务日志（`read_service_logs`），这里直接传入日志行，
/// 由 Rust 侧按错误特征提取引用并做证据式归属；未定位到具体插件时 `plugins` 为空。
#[tauri::command]
pub fn detect_plugin_recovery(
    app_handle: AppHandle,
    logs: Vec<String>,
) -> plugin::PluginRecoveryInfo {
    plugin::detect_recovery(&app_handle, &logs)
}

/// 修复模式卸载单个插件：直接改 profile 清单（离线、精准），成功后推送新插件列表。
///
/// 与 `remove_dsh_plugin`（走 `dsh plugin remove`）不同，此命令不依赖网络，专用于
/// 「插件异常修复」场景；前端随后 `restart()` 重启并重新检测。
#[tauri::command]
pub fn recover_plugin(app_handle: AppHandle, id: String) -> Result<(), String> {
    if plugin::system::is_system_plugin(&id) {
        return Err("PLUGIN_SYSTEM_MANAGED: MIR3 Core Plugin cannot be removed".to_string());
    }
    plugin::uninstall_recovery(&app_handle, &id)?;
    plugin::watch::force_emit(&app_handle);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn governed_pack_transition_compensates_every_failure_and_reports_restore_errors() {
        let compensated = std::cell::Cell::new(false);
        let error = finish_governed_domain_pack_transition::<()>(
            Err("DOMAIN_PACK_RUNTIME_CANARY_FAILED: mismatch".to_string()),
            || {
                compensated.set(true);
                Ok(())
            },
        )
        .unwrap_err();
        assert!(compensated.get());
        assert_eq!(error, "DOMAIN_PACK_RUNTIME_CANARY_FAILED: mismatch");

        let combined = finish_governed_domain_pack_transition::<()>(
            Err("DOMAIN_PACK_ROLLBACK_CANARY_FAILED: mismatch".to_string()),
            || Err("GOVERNANCE_RESTORE_FAILED: locked".to_string()),
        )
        .unwrap_err();
        assert_eq!(
            combined,
            "DOMAIN_PACK_ROLLBACK_CANARY_FAILED: mismatch; governance_restore=GOVERNANCE_RESTORE_FAILED: locked"
        );
    }

    #[test]
    fn governed_pack_transition_never_runs_compensation_after_success() {
        let result = finish_governed_domain_pack_transition(Ok("stable"), || {
            panic!("successful transition must not restore the previous governance snapshot")
        })
        .unwrap();
        assert_eq!(result, "stable");
    }
}
