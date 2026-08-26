//! 996 项目领域服务。
//!
//! Tauri 与 MCP 共享 `mir3-domain`，本模块只管理桌面生命周期相关的扫描任务和
//! Draft 一次性确认令牌，不复制领域规则。

use mir3_domain::{DomainStore, DraftPreview, ScanSummary};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

pub mod canary;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanState {
    pub project_id: Option<String>,
    pub phase: ScanPhase,
    pub summary: Option<ScanSummary>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScanPhase {
    Idle,
    Running,
    Completed,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftConfirmation {
    pub preview: DraftPreview,
    pub confirmation_token: String,
}

#[derive(Debug, Clone)]
struct ConfirmationBinding {
    project_id: String,
    draft_id: String,
    revision: i64,
    diff_hash: String,
}

#[derive(Clone)]
pub struct ProjectService {
    store: DomainStore,
    scan_cancelled: Arc<AtomicBool>,
    scan_state: Arc<Mutex<ScanState>>,
    confirmations: Arc<Mutex<HashMap<String, ConfirmationBinding>>>,
    confirmation_nonce: Arc<AtomicU64>,
}

impl ProjectService {
    #[cfg(test)]
    pub fn new(data_root: PathBuf) -> Result<Self, String> {
        let domain_pack_root = data_root.join("domain-packs");
        Self::new_with_domain_pack_root(data_root, domain_pack_root)
    }

    pub fn new_with_domain_pack_root(
        data_root: PathBuf,
        domain_pack_root: PathBuf,
    ) -> Result<Self, String> {
        Ok(Self {
            store: DomainStore::new_with_domain_pack_root(data_root, domain_pack_root)?,
            scan_cancelled: Arc::new(AtomicBool::new(false)),
            scan_state: Arc::new(Mutex::new(ScanState {
                project_id: None,
                phase: ScanPhase::Idle,
                summary: None,
                error: None,
            })),
            confirmations: Arc::new(Mutex::new(HashMap::new())),
            confirmation_nonce: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn store(&self) -> &DomainStore {
        &self.store
    }

    pub fn scan_state(&self) -> ScanState {
        self.scan_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn cancel_scan(&self, project_id: &str) -> Result<ScanState, String> {
        let state = self.scan_state();
        if state.phase != ScanPhase::Running || state.project_id.as_deref() != Some(project_id) {
            return Err("SCAN_NOT_RUNNING: no matching scan is running".to_string());
        }
        self.scan_cancelled.store(true, Ordering::SeqCst);
        Ok(state)
    }

    pub fn start_scan(&self, app: AppHandle, project_id: String) -> Result<ScanState, String> {
        {
            let mut state = self
                .scan_state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.phase == ScanPhase::Running {
                if state.project_id.as_deref() == Some(project_id.as_str()) {
                    return Ok(state.clone());
                }
                return Err("SCAN_BUSY: another project is being scanned".to_string());
            }
            *state = ScanState {
                project_id: Some(project_id.clone()),
                phase: ScanPhase::Running,
                summary: None,
                error: None,
            };
        }
        self.scan_cancelled.store(false, Ordering::SeqCst);
        let service = self.clone();
        let initial = service.scan_state();
        let _ = app.emit("mir3-scan-updated", &initial);
        std::thread::spawn(move || {
            let result = service.store.scan_project(&project_id, || {
                service.scan_cancelled.load(Ordering::SeqCst)
            });
            let mut state = service
                .scan_state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *state = match result {
                Ok(summary) => ScanState {
                    project_id: Some(project_id),
                    phase: if summary.cancelled {
                        ScanPhase::Cancelled
                    } else {
                        ScanPhase::Completed
                    },
                    summary: Some(summary),
                    error: None,
                },
                Err(error) => ScanState {
                    project_id: Some(project_id),
                    phase: ScanPhase::Error,
                    summary: None,
                    error: Some(error),
                },
            };
            let _ = app.emit("mir3-scan-updated", &*state);
        });
        Ok(initial)
    }

    pub fn create_confirmation(
        &self,
        project_id: &str,
        draft_id: &str,
    ) -> Result<DraftConfirmation, String> {
        let preview = self.store.preview_draft(project_id, draft_id)?;
        let nonce = self.confirmation_nonce.fetch_add(1, Ordering::SeqCst) + 1;
        let mut hasher = Sha256::new();
        hasher.update(project_id.as_bytes());
        hasher.update(draft_id.as_bytes());
        hasher.update(preview.draft.revision.to_le_bytes());
        hasher.update(preview.diff_hash.as_bytes());
        hasher.update(nonce.to_le_bytes());
        let token = format!("{:x}", hasher.finalize());
        self.confirmations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                token.clone(),
                ConfirmationBinding {
                    project_id: project_id.to_string(),
                    draft_id: draft_id.to_string(),
                    revision: preview.draft.revision,
                    diff_hash: preview.diff_hash.clone(),
                },
            );
        Ok(DraftConfirmation {
            preview,
            confirmation_token: token,
        })
    }

    pub fn consume_confirmation(
        &self,
        project_id: &str,
        draft_id: &str,
        token: &str,
    ) -> Result<ConfirmationValues, String> {
        let binding = self
            .confirmations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(token)
            .ok_or_else(|| {
                "DRAFT_CONFIRMATION_INVALID: confirmation token is invalid or already used"
                    .to_string()
            })?;
        if binding.project_id != project_id || binding.draft_id != draft_id {
            return Err("DRAFT_CONFIRMATION_INVALID: token belongs to another draft".to_string());
        }
        Ok(ConfirmationValues {
            revision: binding.revision,
            diff_hash: binding.diff_hash,
        })
    }

    /// 组合确认先完整核对全部一次性令牌，再在同一把锁内整体消费。
    pub fn consume_composite_confirmations(
        &self,
        project_id: &str,
        requests: &[(String, String)],
    ) -> Result<Vec<ConfirmationValues>, String> {
        let mut confirmations = self
            .confirmations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut values = Vec::with_capacity(requests.len());
        let mut tokens = Vec::with_capacity(requests.len());
        let mut draft_ids = Vec::with_capacity(requests.len());
        for (draft_id, token) in requests {
            if tokens.contains(token) || draft_ids.contains(draft_id) {
                return Err(
                    "COMPOSITE_CONFIRMATION_DUPLICATE: Drafts and confirmation tokens must be unique"
                        .to_string(),
                );
            }
            let binding = confirmations.get(token).ok_or_else(|| {
                "DRAFT_CONFIRMATION_INVALID: confirmation token is invalid or already used"
                    .to_string()
            })?;
            if binding.project_id != project_id || binding.draft_id != *draft_id {
                return Err(
                    "DRAFT_CONFIRMATION_INVALID: token belongs to another draft".to_string()
                );
            }
            values.push(ConfirmationValues {
                revision: binding.revision,
                diff_hash: binding.diff_hash.clone(),
            });
            tokens.push(token.clone());
            draft_ids.push(draft_id.clone());
        }
        for token in tokens {
            confirmations.remove(&token);
        }
        Ok(values)
    }
}

#[derive(Debug)]
pub struct ConfirmationValues {
    pub revision: i64,
    pub diff_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_confirmation_failure_consumes_no_valid_tokens() {
        let base = std::env::temp_dir().join(format!(
            "mir3-composite-confirmation-{}-{}",
            std::process::id(),
            mir3_domain::now_millis()
        ));
        let root = base.join("组合确认项目");
        std::fs::create_dir_all(root.join("客户端/dev")).unwrap();
        std::fs::create_dir_all(root.join("引擎")).unwrap();
        let service = ProjectService::new(base.join("data")).unwrap();
        let project = service.store().import_project(&root).unwrap();
        let first = service.store().open_draft(&project.id, "first").unwrap();
        let second = service.store().open_draft(&project.id, "second").unwrap();
        let first_confirmation = service.create_confirmation(&project.id, &first.id).unwrap();
        let error = service
            .consume_composite_confirmations(
                &project.id,
                &[
                    (
                        first.id.clone(),
                        first_confirmation.confirmation_token.clone(),
                    ),
                    (second.id, "invalid-token".to_string()),
                ],
            )
            .unwrap_err();
        assert!(error.starts_with("DRAFT_CONFIRMATION_INVALID:"));
        service
            .consume_confirmation(
                &project.id,
                &first.id,
                &first_confirmation.confirmation_token,
            )
            .unwrap();
        std::fs::remove_dir_all(base).ok();
    }
}

/// 开发与发布环境下定位 Rust MCP sidecar。
pub fn mcp_binary_path(app: &AppHandle) -> Option<PathBuf> {
    let binary = if cfg!(windows) {
        "mir3-mcp.exe"
    } else {
        "mir3-mcp"
    };
    let mut candidates = Vec::new();
    if let Ok(resource) = app.path().resource_dir() {
        candidates.push(resource.join(binary));
        candidates.push(resource.join("binaries").join(binary));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("debug")
            .join(binary),
    );
    candidates.into_iter().find(|path| path.is_file())
}
