#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerRoute {
    StartContractRun { contract_id: String },
    GetContractRunStatus { run_id: String },
    GetContractRunNext { run_id: String },
    ExecuteContractRunStep { run_id: String, step_id: String },
    GetCurrentGate { run_id: String },
    ResolveGate { run_id: String, gate_id: String },
    GetContractRunEvidence { run_id: String },
}

pub fn match_worker_route(method: &str, path: &str) -> Option<WorkerRoute> {
    let trimmed = path.trim_matches('/');
    let segments = if trimmed.is_empty() {
        Vec::new()
    } else {
        trimmed.split('/').collect::<Vec<_>>()
    };

    match (method, segments.as_slice()) {
        ("POST", ["contracts", contract_id, "runs"]) => Some(WorkerRoute::StartContractRun {
            contract_id: (*contract_id).to_string(),
        }),
        ("GET", ["contract-runs", run_id]) => Some(WorkerRoute::GetContractRunStatus {
            run_id: (*run_id).to_string(),
        }),
        ("GET", ["contract-runs", run_id, "next"]) => Some(WorkerRoute::GetContractRunNext {
            run_id: (*run_id).to_string(),
        }),
        ("POST", ["contract-runs", run_id, "steps", step_id, "execute"]) => {
            Some(WorkerRoute::ExecuteContractRunStep {
                run_id: (*run_id).to_string(),
                step_id: (*step_id).to_string(),
            })
        }
        ("GET", ["contract-runs", run_id, "gates", "current"]) => Some(WorkerRoute::GetCurrentGate {
            run_id: (*run_id).to_string(),
        }),
        ("POST", ["contract-runs", run_id, "gates", gate_id, "resolve"]) => Some(WorkerRoute::ResolveGate {
            run_id: (*run_id).to_string(),
            gate_id: (*gate_id).to_string(),
        }),
        ("GET", ["contract-runs", run_id, "evidence"]) => Some(WorkerRoute::GetContractRunEvidence {
            run_id: (*run_id).to_string(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{match_worker_route, WorkerRoute};

    #[test]
    fn matches_contract_run_start_route() {
        assert_eq!(
            match_worker_route("POST", "/contracts/fixture_contract_run_alpha_v0/runs"),
            Some(WorkerRoute::StartContractRun {
                contract_id: "fixture_contract_run_alpha_v0".to_string(),
            })
        );
    }

    #[test]
    fn matches_contract_run_status_route() {
        assert_eq!(
            match_worker_route("GET", "/contract-runs/run_001"),
            Some(WorkerRoute::GetContractRunStatus {
                run_id: "run_001".to_string(),
            })
        );
    }

    #[test]
    fn matches_contract_run_next_route() {
        assert_eq!(
            match_worker_route("GET", "/contract-runs/run_001/next"),
            Some(WorkerRoute::GetContractRunNext {
                run_id: "run_001".to_string(),
            })
        );
    }

    #[test]
    fn matches_contract_run_execute_route() {
        assert_eq!(
            match_worker_route("POST", "/contract-runs/run_001/steps/step_001/execute"),
            Some(WorkerRoute::ExecuteContractRunStep {
                run_id: "run_001".to_string(),
                step_id: "step_001".to_string(),
            })
        );
    }

    #[test]
    fn matches_contract_run_gate_current_route() {
        assert_eq!(
            match_worker_route("GET", "/contract-runs/run_001/gates/current"),
            Some(WorkerRoute::GetCurrentGate {
                run_id: "run_001".to_string(),
            })
        );
    }

    #[test]
    fn matches_contract_run_gate_resolve_route() {
        assert_eq!(
            match_worker_route("POST", "/contract-runs/run_001/gates/gate_001/resolve"),
            Some(WorkerRoute::ResolveGate {
                run_id: "run_001".to_string(),
                gate_id: "gate_001".to_string(),
            })
        );
    }

    #[test]
    fn matches_contract_run_evidence_route() {
        assert_eq!(
            match_worker_route("GET", "/contract-runs/run_001/evidence"),
            Some(WorkerRoute::GetContractRunEvidence {
                run_id: "run_001".to_string(),
            })
        );
    }

    #[test]
    fn does_not_match_missing_adapter_route() {
        assert_eq!(match_worker_route("GET", "/contract-runs"), None);
        assert_eq!(match_worker_route("POST", "/contracts/fixture_contract_run_alpha_v0/run"), None);
    }
}
