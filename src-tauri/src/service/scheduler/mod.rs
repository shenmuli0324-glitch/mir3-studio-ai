use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::time;

const DOMAIN_UPDATE_INITIAL_DELAY: Duration = Duration::from_secs(60);
const DOMAIN_UPDATE_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const CORE_STARTING_INTERVAL: Duration = Duration::from_secs(1);
const CORE_STEADY_INTERVAL: Duration = Duration::from_secs(5);
const EXTERNAL_STATE_INTERVAL: Duration = Duration::from_secs(5);

pub fn start(app_handle: &AppHandle) {
    log::info!("Starting dsh process monitor");
    let app_handle_clone = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        core_monitor_loop(app_handle_clone).await;
    });
    let app_handle_clone = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        external_state_loop(app_handle_clone).await;
    });
    let app_handle_clone = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        domain_update_permanent_loop(app_handle_clone).await;
    });
}

/// 正式更新源启用时，后台只发现并暂存已验签候选；激活仍只能由用户确认入口完成。
async fn domain_update_permanent_loop(app_handle: AppHandle) {
    if !crate::service::plugin::domain_update::is_configured() {
        log::info!("MIR3 domain-pack remote updates are not configured; background check disabled");
        return;
    }
    time::sleep(DOMAIN_UPDATE_INITIAL_DELAY).await;
    let mut interval = time::interval(DOMAIN_UPDATE_INTERVAL);
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        if let Err(error) = check_and_stage_domain_candidates(&app_handle).await {
            log::warn!("MIR3 domain-pack background candidate check failed: {error}");
        }
    }
}

async fn check_and_stage_domain_candidates(app_handle: &AppHandle) -> Result<(), String> {
    let root = crate::config::get_dsh_data_path(app_handle).join("domain-packs");
    let updates = crate::service::plugin::domain_update::check(&root, None).await?;
    let mut staged = Vec::new();
    let mut selected_systems = std::collections::BTreeSet::new();
    for update in updates.updates {
        // 索引可保留多个历史发行版；check 已按版本降序，只暂存每个系统最新项。
        if !selected_systems.insert(update.system_id.clone()) {
            continue;
        }
        let state = crate::service::plugin::system::domain_pack_state(&root, &update.system_id)?;
        if state
            .state
            .candidate
            .as_ref()
            .is_some_and(|candidate| candidate.version == update.version)
        {
            continue;
        }
        match crate::service::plugin::domain_update::stage(
            &root,
            &update.system_id,
            &update.version,
        )
        .await
        {
            Ok(_) => staged.push(update.system_id),
            Err(error) => log::warn!(
                "MIR3 domain-pack candidate staging failed for {}@{}: {error}",
                update.system_id,
                update.version
            ),
        }
    }
    if !staged.is_empty() {
        app_handle
            .emit("domain-pack-candidates-updated", &staged)
            .map_err(|error| format!("DOMAIN_PACK_UPDATE_EVENT_FAILED: {error}"))?;
        log::info!("Staged MIR3 domain-pack candidates: {}", staged.join(", "));
    }
    Ok(())
}

async fn core_monitor_loop(app_handle: AppHandle) {
    loop {
        if let Err(e) = crate::task::tick_check_dsh_process::trigger(app_handle.clone()).await {
            log::warn!("tick_check_dsh_process failed: {e}");
        }
        time::sleep(core_poll_interval(
            crate::service::workflow::status::get_status(),
        ))
        .await;
    }
}

/// Core 启动期间需要尽快确认 ready；稳定运行后降低 HTTP 探测频率。
fn core_poll_interval(status: crate::service::workflow::status::Status) -> Duration {
    if status == crate::service::workflow::status::Status::Starting {
        CORE_STARTING_INTERVAL
    } else {
        CORE_STEADY_INTERVAL
    }
}

/// 主题与插件也可能被外部进程修改，因此保留低频元数据兜底轮询。
async fn external_state_loop(app_handle: AppHandle) {
    let mut interval = time::interval(EXTERNAL_STATE_INTERVAL);
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        crate::config::check_and_emit_theme(&app_handle);
        crate::service::plugin::watch::check_and_emit(&app_handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::workflow::status::Status;

    #[test]
    fn core_polling_is_fast_only_while_starting() {
        assert_eq!(core_poll_interval(Status::Starting), Duration::from_secs(1));
        assert_eq!(core_poll_interval(Status::Running), Duration::from_secs(5));
        assert_eq!(core_poll_interval(Status::Stopped), Duration::from_secs(5));
    }
}
