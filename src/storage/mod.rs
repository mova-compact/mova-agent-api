//! Storage adapter boundary for MOVA Agent API V0 persistence promotion.
//!
//! This module keeps storage provider-agnostic and exposes deterministic
//! in-memory and local file-backed adapters.

use crate::evidence::EvidenceResponse;
use crate::observation::ObservationRecord;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSnapshot {
    pub run_id: String,
    pub evidence: EvidenceResponse,
    pub observations: Vec<ObservationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageError {
    pub code: String,
    pub message: String,
}

impl StorageError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
        }
    }
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
pub trait RunStore: Send + Sync {
    async fn put_snapshot(&self, snapshot: RunSnapshot) -> Result<(), StorageError>;
    async fn get_snapshot(&self, run_id: &str) -> Result<Option<RunSnapshot>, StorageError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageConfig {
    pub adapter_kind: String,
    pub base_path: Option<String>,
}

impl StorageConfig {
    pub fn in_memory_default() -> Self {
        Self {
            adapter_kind: "in_memory".to_string(),
            base_path: None,
        }
    }

    pub fn file_backed_local(base_path: String) -> Self {
        Self {
            adapter_kind: "file_backed_local".to_string(),
            base_path: Some(base_path),
        }
    }

    pub fn validate(&self) -> Result<(), StorageError> {
        if self.adapter_kind == "in_memory" {
            return Ok(());
        }
        if self.adapter_kind == "file_backed_local" {
            if self.base_path.as_deref().unwrap_or("").trim().is_empty() {
                return Err(StorageError::new(
                    "storage_config_invalid",
                    "base_path is required for file_backed_local",
                ));
            }
            return Ok(());
        }
        Err(StorageError::new(
            "storage_config_invalid",
            "unsupported adapter_kind",
        ))
    }
}

#[derive(Debug, Default)]
pub struct InMemoryRunStore {
    runs: Arc<Mutex<HashMap<String, RunSnapshot>>>,
}

impl InMemoryRunStore {
    pub fn new() -> Self {
        Self {
            runs: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl RunStore for InMemoryRunStore {
    async fn put_snapshot(&self, snapshot: RunSnapshot) -> Result<(), StorageError> {
        let mut guard = self
            .runs
            .lock()
            .map_err(|_| StorageError::new("storage_lock_failed", "in-memory store lock poisoned"))?;
        guard.insert(snapshot.run_id.clone(), snapshot);
        Ok(())
    }

    async fn get_snapshot(&self, run_id: &str) -> Result<Option<RunSnapshot>, StorageError> {
        let guard = self
            .runs
            .lock()
            .map_err(|_| StorageError::new("storage_lock_failed", "in-memory store lock poisoned"))?;
        Ok(guard.get(run_id).cloned())
    }
}

#[derive(Debug, Clone)]
pub struct FileBackedRunStore {
    base_dir: PathBuf,
}

impl FileBackedRunStore {
    pub fn new(base_dir: PathBuf) -> Result<Self, StorageError> {
        fs::create_dir_all(&base_dir)
            .map_err(|err| StorageError::new("storage_init_failed", &format!("create_dir_all failed: {err}")))?;
        Ok(Self { base_dir })
    }

    fn run_path(&self, run_id: &str) -> PathBuf {
        self.base_dir.join(format!("{run_id}.json"))
    }
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl RunStore for FileBackedRunStore {
    async fn put_snapshot(&self, snapshot: RunSnapshot) -> Result<(), StorageError> {
        let path = self.run_path(&snapshot.run_id);
        let bytes = serde_json::to_vec_pretty(&snapshot)
            .map_err(|err| StorageError::new("storage_serialize_failed", &format!("serialize failed: {err}")))?;
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, bytes)
            .map_err(|err| StorageError::new("storage_write_failed", &format!("write failed: {err}")))?;
        fs::rename(&tmp_path, &path)
            .map_err(|err| StorageError::new("storage_write_failed", &format!("rename failed: {err}")))?;
        Ok(())
    }

    async fn get_snapshot(&self, run_id: &str) -> Result<Option<RunSnapshot>, StorageError> {
        let path = self.run_path(run_id);
        if !Path::new(&path).exists() {
            return Ok(None);
        }
        let data = fs::read(&path)
            .map_err(|err| StorageError::new("storage_read_failed", &format!("read failed: {err}")))?;
        let snapshot: RunSnapshot = serde_json::from_slice(&data)
            .map_err(|err| StorageError::new("storage_deserialize_failed", &format!("deserialize failed: {err}")))?;
        Ok(Some(snapshot))
    }
}

pub fn create_run_store(config: &StorageConfig) -> Box<dyn RunStore> {
    match config.validate() {
        Ok(()) if config.adapter_kind == "in_memory" => Box::new(InMemoryRunStore::new()),
        Ok(()) if config.adapter_kind == "file_backed_local" => {
            let path = PathBuf::from(config.base_path.as_deref().unwrap_or_default());
            match FileBackedRunStore::new(path) {
                Ok(store) => Box::new(store),
                Err(err) => Box::new(FailingRunStore::new(err)),
            }
        }
        Ok(()) => Box::new(FailingRunStore::new(StorageError::new(
            "storage_config_invalid",
            "unsupported adapter_kind",
        ))),
        Err(err) => Box::new(FailingRunStore::new(err)),
    }
}

#[cfg(feature = "cloudflare_worker")]
pub struct CloudflareKvRunStore {
    kv: worker::kv::KvStore,
    ttl_seconds: Option<u64>,
}

#[cfg(feature = "cloudflare_worker")]
impl CloudflareKvRunStore {
    pub fn new(kv: worker::kv::KvStore) -> Self {
        Self {
            kv,
            ttl_seconds: None,
        }
    }

    pub fn new_with_ttl(kv: worker::kv::KvStore, ttl_seconds: Option<u64>) -> Self {
        Self { kv, ttl_seconds }
    }

    fn key_for(run_id: &str) -> String {
        format!("run_snapshot:{run_id}")
    }
}

#[cfg(feature = "cloudflare_worker")]
#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl RunStore for CloudflareKvRunStore {
    async fn put_snapshot(&self, snapshot: RunSnapshot) -> Result<(), StorageError> {
        let key = Self::key_for(&snapshot.run_id);
        let body = serde_json::to_string(&snapshot)
            .map_err(|err| StorageError::new("storage_serialize_failed", &format!("serialize failed: {err}")))?;
        let mut put = self
            .kv
            .put(&key, body)
            .map_err(|err| StorageError::new("storage_write_failed", &format!("kv put build failed: {err}")))?;
        if let Some(ttl) = self.ttl_seconds {
            put = put.expiration_ttl(ttl);
        }
        put.execute()
            .await
            .map_err(|err| StorageError::new("storage_write_failed", &format!("kv put execute failed: {err}")))?;
        Ok(())
    }

    async fn get_snapshot(&self, run_id: &str) -> Result<Option<RunSnapshot>, StorageError> {
        let key = Self::key_for(run_id);
        let data = self
            .kv
            .get(&key)
            .json::<RunSnapshot>()
            .await
            .map_err(|err| StorageError::new("storage_read_failed", &format!("kv get failed: {err}")))?;
        Ok(data)
    }
}

#[derive(Debug, Clone)]
pub struct FailingRunStore {
    err: StorageError,
}

impl FailingRunStore {
    pub fn new(err: StorageError) -> Self {
        Self { err }
    }
}

#[cfg_attr(all(feature = "cloudflare_worker", target_arch = "wasm32"), async_trait(?Send))]
#[cfg_attr(not(all(feature = "cloudflare_worker", target_arch = "wasm32")), async_trait)]
impl RunStore for FailingRunStore {
    async fn put_snapshot(&self, _snapshot: RunSnapshot) -> Result<(), StorageError> {
        Err(self.err.clone())
    }

    async fn get_snapshot(&self, _run_id: &str) -> Result<Option<RunSnapshot>, StorageError> {
        Err(self.err.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::RunStatus;
    use crate::policy::{AdmissionDecision, PolicySummary};

    fn sample_snapshot(run_id: &str) -> RunSnapshot {
        RunSnapshot {
            run_id: run_id.to_string(),
            evidence: EvidenceResponse {
                run_id: run_id.to_string(),
                status: RunStatus::Completed,
                result: serde_json::json!({"outcome": "skeleton"}),
                evidence: serde_json::json!({"record_count": 1}),
                trace_ref: format!("trace:{run_id}"),
                observation_refs: vec!["ev_01".to_string()],
                policy_summary: PolicySummary {
                    decision: AdmissionDecision::Allow,
                    policy_version: "policy.default.v0".to_string(),
                    reason_code: "authorized".to_string(),
                },
            },
            observations: vec![ObservationRecord {
                run_id: run_id.to_string(),
                step_id: "step_observation_write".to_string(),
                event_type: "observation.write".to_string(),
                timestamp: "2026-05-23T10:30:01Z".to_string(),
                subject: serde_json::json!({"call_id": "call_01"}),
                result: serde_json::json!({"status": "ok"}),
                metadata: serde_json::json!({}),
                evidence_ref: "ev_01".to_string(),
            }],
        }
    }

    #[tokio::test]
    async fn in_memory_store_roundtrip_snapshot() {
        let store = InMemoryRunStore::new();
        let snapshot = sample_snapshot("run_mem_01");
        store.put_snapshot(snapshot.clone()).await.unwrap();
        let read = store.get_snapshot("run_mem_01").await.unwrap().unwrap();
        assert_eq!(read.run_id, "run_mem_01");
        assert_eq!(read.evidence.trace_ref, "trace:run_mem_01");
    }

    #[tokio::test]
    async fn file_backed_store_roundtrip_snapshot() {
        let unique = format!(
            "mova_agent_api_storage_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        let store = FileBackedRunStore::new(dir.clone()).unwrap();
        let snapshot = sample_snapshot("run_file_01");
        store.put_snapshot(snapshot).await.unwrap();
        let read = store.get_snapshot("run_file_01").await.unwrap().unwrap();
        assert_eq!(read.run_id, "run_file_01");
        assert_eq!(read.observations.len(), 1);
        let _ = fs::remove_dir_all(dir);
    }
}
