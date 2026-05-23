//! Observation skeleton for MOVA Agent API V0.
//!
//! Phase 5 scope:
//! - observation record structures
//! - deterministic local append behavior

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservationRecord {
    pub run_id: String,
    pub step_id: String,
    pub event_type: String,
    pub timestamp: String,
    pub subject: Value,
    pub result: Value,
    pub metadata: Value,
    pub evidence_ref: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ObservationJournal {
    records: Vec<ObservationRecord>,
}

impl ObservationJournal {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn append(&mut self, record: ObservationRecord) {
        self.records.push(record);
    }

    pub fn records(&self) -> &[ObservationRecord] {
        &self.records
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_journal_appends_records() {
        let mut journal = ObservationJournal::new();
        let record = ObservationRecord {
            run_id: "run_01".to_string(),
            step_id: "step_observation_write".to_string(),
            event_type: "observation.write".to_string(),
            timestamp: "2026-05-23T09:10:00Z".to_string(),
            subject: serde_json::json!({"action_id": "act_01"}),
            result: serde_json::json!({"status": "ok"}),
            metadata: serde_json::json!({}),
            evidence_ref: "ev_01".to_string(),
        };
        journal.append(record);
        assert_eq!(journal.records().len(), 1);
        assert_eq!(journal.records()[0].run_id, "run_01");
        assert_eq!(journal.records()[0].event_type, "observation.write");
    }
}

