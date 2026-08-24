use crate::{now_millis, DomainStore};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KnowledgeStatus {
    Proposed,
    Active,
    Contested,
    Superseded,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRecord {
    pub id: String,
    pub status: KnowledgeStatus,
    pub kind: String,
    pub summary: String,
    pub body: String,
    pub engine_version: Option<String>,
    pub evidence: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeFilter {
    pub text: Option<String>,
    pub statuses: Vec<KnowledgeStatus>,
    pub limit: Option<usize>,
}

impl DomainStore {
    pub fn create_knowledge(
        &self,
        project_id: &str,
        kind: &str,
        summary: &str,
        body: &str,
        engine_version: Option<&str>,
        evidence: &[String],
    ) -> Result<KnowledgeRecord, String> {
        if summary.trim().is_empty() {
            return Err("KNOWLEDGE_SUMMARY_EMPTY: summary is required".to_string());
        }
        let now = now_millis();
        let mut hasher = Sha256::new();
        hasher.update(project_id.as_bytes());
        hasher.update(summary.as_bytes());
        hasher.update(now.to_le_bytes());
        let id = format!("KC-{:x}", hasher.finalize())[..19].to_string();
        let record = KnowledgeRecord {
            id,
            status: KnowledgeStatus::Proposed,
            kind: kind.trim().to_string(),
            summary: summary.trim().to_string(),
            body: body.trim().to_string(),
            engine_version: engine_version.map(str::to_string),
            evidence: evidence.to_vec(),
            created_at: now,
            updated_at: now,
        };
        self.project_connection(project_id)?
            .execute(
                "INSERT INTO knowledge(id,status,kind,summary,body,engine_version,evidence,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    record.id,
                    knowledge_status_string(record.status),
                    record.kind,
                    record.summary,
                    record.body,
                    record.engine_version,
                    serde_json::to_string(&record.evidence).unwrap_or_else(|_| "[]".to_string()),
                    record.created_at,
                    record.updated_at,
                ],
            )
            .map_err(|e| format!("KNOWLEDGE_CREATE_FAILED: {e}"))?;
        Ok(record)
    }

    pub fn list_knowledge(
        &self,
        project_id: &str,
        filter: &KnowledgeFilter,
    ) -> Result<Vec<KnowledgeRecord>, String> {
        let connection = self.project_connection(project_id)?;
        let text = filter.text.as_deref().unwrap_or("").trim();
        let pattern = format!("%{}%", text.replace('%', "\\%").replace('_', "\\_"));
        let limit = filter.limit.unwrap_or(100).clamp(1, 500) as i64;
        let mut statement = connection
            .prepare(
                "SELECT id,status,kind,summary,body,engine_version,evidence,created_at,updated_at FROM knowledge
                 WHERE (?1='' OR summary LIKE ?2 ESCAPE '\\' OR body LIKE ?2 ESCAPE '\\')
                 ORDER BY updated_at DESC LIMIT ?3",
            )
            .map_err(|e| format!("KNOWLEDGE_LIST_FAILED: {e}"))?;
        let rows = statement
            .query_map(params![text, pattern, limit], row_to_knowledge)
            .map_err(|e| format!("KNOWLEDGE_LIST_FAILED: {e}"))?;
        let mut records = Vec::new();
        for row in rows {
            let record = row.map_err(|e| format!("KNOWLEDGE_LIST_FAILED: {e}"))?;
            if filter.statuses.is_empty() || filter.statuses.contains(&record.status) {
                records.push(record);
            }
        }
        Ok(records)
    }

    pub fn get_knowledge(
        &self,
        project_id: &str,
        knowledge_id: &str,
    ) -> Result<KnowledgeRecord, String> {
        self.project_connection(project_id)?
            .query_row(
                "SELECT id,status,kind,summary,body,engine_version,evidence,created_at,updated_at FROM knowledge WHERE id=?1",
                [knowledge_id],
                row_to_knowledge,
            )
            .optional()
            .map_err(|e| format!("KNOWLEDGE_GET_FAILED: {e}"))?
            .ok_or_else(|| format!("KNOWLEDGE_NOT_FOUND: {knowledge_id}"))
    }

    pub fn set_knowledge_status(
        &self,
        project_id: &str,
        knowledge_id: &str,
        status: KnowledgeStatus,
    ) -> Result<KnowledgeRecord, String> {
        let current = self.get_knowledge(project_id, knowledge_id)?;
        if !valid_transition(current.status, status) {
            return Err(format!(
                "KNOWLEDGE_TRANSITION_INVALID: {:?} cannot transition to {:?}",
                current.status, status
            ));
        }
        self.project_connection(project_id)?
            .execute(
                "UPDATE knowledge SET status=?2,updated_at=?3 WHERE id=?1",
                params![knowledge_id, knowledge_status_string(status), now_millis()],
            )
            .map_err(|e| format!("KNOWLEDGE_UPDATE_FAILED: {e}"))?;
        self.get_knowledge(project_id, knowledge_id)
    }

    /// MCP 专用检索：只返回已激活且与当前引擎版本兼容的知识。
    pub fn search_active_knowledge(
        &self,
        project_id: &str,
        text: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeRecord>, String> {
        let project = self.get_project(project_id)?;
        let filter = KnowledgeFilter {
            text: Some(text.to_string()),
            statuses: vec![KnowledgeStatus::Active],
            limit: Some(limit),
        };
        Ok(self
            .list_knowledge(project_id, &filter)?
            .into_iter()
            .filter(|record| {
                record.engine_version.is_none()
                    || project.engine_version.is_none()
                    || record.engine_version == project.engine_version
            })
            .collect())
    }
}

fn row_to_knowledge(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeRecord> {
    let status: String = row.get(1)?;
    let evidence: String = row.get(6)?;
    Ok(KnowledgeRecord {
        id: row.get(0)?,
        status: parse_knowledge_status(&status),
        kind: row.get(2)?,
        summary: row.get(3)?,
        body: row.get(4)?,
        engine_version: row.get(5)?,
        evidence: serde_json::from_str(&evidence).unwrap_or_default(),
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn knowledge_status_string(status: KnowledgeStatus) -> &'static str {
    match status {
        KnowledgeStatus::Proposed => "PROPOSED",
        KnowledgeStatus::Active => "ACTIVE",
        KnowledgeStatus::Contested => "CONTESTED",
        KnowledgeStatus::Superseded => "SUPERSEDED",
        KnowledgeStatus::Revoked => "REVOKED",
    }
}

fn parse_knowledge_status(value: &str) -> KnowledgeStatus {
    match value {
        "ACTIVE" => KnowledgeStatus::Active,
        "CONTESTED" => KnowledgeStatus::Contested,
        "SUPERSEDED" => KnowledgeStatus::Superseded,
        "REVOKED" => KnowledgeStatus::Revoked,
        _ => KnowledgeStatus::Proposed,
    }
}

fn valid_transition(from: KnowledgeStatus, to: KnowledgeStatus) -> bool {
    from == to
        || matches!(
            (from, to),
            (KnowledgeStatus::Proposed, KnowledgeStatus::Active)
                | (KnowledgeStatus::Proposed, KnowledgeStatus::Revoked)
                | (KnowledgeStatus::Active, KnowledgeStatus::Contested)
                | (KnowledgeStatus::Active, KnowledgeStatus::Superseded)
                | (KnowledgeStatus::Active, KnowledgeStatus::Revoked)
                | (KnowledgeStatus::Contested, KnowledgeStatus::Active)
                | (KnowledgeStatus::Contested, KnowledgeStatus::Revoked)
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn mcp_search_returns_only_active_knowledge() {
        let base = std::env::temp_dir().join(format!("mir3-knowledge-{}", std::process::id()));
        let project = base.join("项目");
        fs::create_dir_all(project.join("客户端")).unwrap();
        fs::create_dir_all(project.join("引擎")).unwrap();
        let store = DomainStore::new(base.join("data")).unwrap();
        let imported = store.import_project(&project).unwrap();
        let record = store
            .create_knowledge(
                &imported.id,
                "PROJECT_FACT",
                "NPC入口",
                "Market_Def",
                None,
                &[],
            )
            .unwrap();
        assert!(store
            .search_active_knowledge(&imported.id, "NPC", 10)
            .unwrap()
            .is_empty());
        store
            .set_knowledge_status(&imported.id, &record.id, KnowledgeStatus::Active)
            .unwrap();
        assert_eq!(
            store
                .search_active_knowledge(&imported.id, "NPC", 10)
                .unwrap()
                .len(),
            1
        );
        fs::remove_dir_all(base).ok();
    }
}
