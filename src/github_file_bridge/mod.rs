//! Minimal GitHub-file-compatible local file bridge for barbershop proof.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Appointment {
    pub id: String,
    pub amount: f64,
    #[serde(default)]
    pub staff_id: Option<String>,
    #[serde(default)]
    pub cancelled: bool,
    #[serde(default)]
    pub no_show: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payment {
    pub id: String,
    pub amount: f64,
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyInput {
    pub date: String,
    pub appointments: Vec<Appointment>,
    pub payments: Vec<Payment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsRules {
    pub required_metrics: Vec<String>,
    pub deviation_threshold_pct: f64,
    pub impossible_average_check_max: f64,
    pub require_payments_for_completed_visits: bool,
    pub fallback_behavior: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterministicMetrics {
    pub total_revenue: f64,
    pub visit_count: usize,
    pub average_check: f64,
    pub cancellations: usize,
    pub no_shows: usize,
    pub payment_split: Value,
    pub staff_totals: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubFileE2eResult {
    pub metrics: DeterministicMetrics,
    pub flags: Vec<String>,
    pub require_human_gate: bool,
    pub ai_report_draft: String,
    pub report_path: String,
    pub audit_path: String,
    pub telegram_delivery_result: String,
}

#[derive(Debug, Clone)]
pub struct GitHubFileBridgeError {
    pub code: String,
    pub message: String,
}

impl GitHubFileBridgeError {
    fn new(code: &str, message: String) -> Self {
        Self {
            code: code.to_string(),
            message,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_json_file<T: for<'de> Deserialize<'de>>(path: &str) -> Result<T, GitHubFileBridgeError> {
    let content = std::fs::read_to_string(path).map_err(|err| {
        GitHubFileBridgeError::new("github_file_read_failed", format!("read {} failed: {err}", path))
    })?;
    serde_json::from_str::<T>(&content).map_err(|err| {
        GitHubFileBridgeError::new("github_file_parse_failed", format!("parse {} failed: {err}", path))
    })
}

#[cfg(target_arch = "wasm32")]
fn read_json_file<T: for<'de> Deserialize<'de>>(_path: &str) -> Result<T, GitHubFileBridgeError> {
    Err(GitHubFileBridgeError::new(
        "github_file_unavailable",
        "local github-file bridge is not available on wasm runtime".to_string(),
    ))
}

#[cfg(not(target_arch = "wasm32"))]
fn write_text_file(path: &str, content: &str) -> Result<(), GitHubFileBridgeError> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            GitHubFileBridgeError::new("github_file_write_failed", format!("mkdir {} failed: {err}", parent.display()))
        })?;
    }
    std::fs::write(path, content).map_err(|err| {
        GitHubFileBridgeError::new("github_file_write_failed", format!("write {} failed: {err}", path))
    })
}

#[cfg(target_arch = "wasm32")]
fn write_text_file(_path: &str, _content: &str) -> Result<(), GitHubFileBridgeError> {
    Err(GitHubFileBridgeError::new(
        "github_file_unavailable",
        "local github-file bridge is not available on wasm runtime".to_string(),
    ))
}

pub fn run_barbershop_file_e2e(
    contract_id: &str,
    run_id: &str,
    input_path: &str,
    rules_path: &str,
    report_path: &str,
    audit_path: &str,
    telegram_live_configured: bool,
) -> Result<GitHubFileE2eResult, GitHubFileBridgeError> {
    let input: DailyInput = read_json_file(input_path)?;
    let rules: MetricsRules = read_json_file(rules_path)?;

    let visit_count = input.appointments.len();
    let total_revenue: f64 = input.payments.iter().map(|p| p.amount).sum();
    let average_check = if visit_count == 0 {
        0.0
    } else {
        total_revenue / visit_count as f64
    };
    let cancellations = input.appointments.iter().filter(|a| a.cancelled).count();
    let no_shows = input.appointments.iter().filter(|a| a.no_show).count();

    let mut payment_split = serde_json::Map::new();
    for payment in &input.payments {
        let entry = payment_split.entry(payment.method.clone()).or_insert(json!(0.0));
        let cur = entry.as_f64().unwrap_or(0.0);
        *entry = json!(cur + payment.amount);
    }

    let mut staff_totals_map = serde_json::Map::new();
    for appointment in &input.appointments {
        if let Some(staff_id) = &appointment.staff_id {
            let entry = staff_totals_map.entry(staff_id.clone()).or_insert(json!(0.0));
            let cur = entry.as_f64().unwrap_or(0.0);
            *entry = json!(cur + appointment.amount);
        }
    }

    let metrics = DeterministicMetrics {
        total_revenue,
        visit_count,
        average_check,
        cancellations,
        no_shows,
        payment_split: Value::Object(payment_split),
        staff_totals: Value::Object(staff_totals_map),
    };

    let mut flags = Vec::new();
    if rules.require_payments_for_completed_visits && input.payments.is_empty() && !input.appointments.is_empty() {
        flags.push("missing_payments_for_visits".to_string());
    }
    if average_check > rules.impossible_average_check_max {
        flags.push("impossible_average_check".to_string());
    }
    if no_shows > 0 || cancellations > 0 {
        flags.push("attendance_issues_present".to_string());
    }
    let require_human_gate = !flags.is_empty();

    let ai_report_draft = format!(
        "Owner report for {}: revenue {:.2}, visits {}, average check {:.2}, cancellations {}, no-shows {}.",
        input.date, metrics.total_revenue, metrics.visit_count, metrics.average_check, metrics.cancellations, metrics.no_shows
    );
    let telegram_delivery_result = if telegram_live_configured {
        "telegram_delivery_result".to_string()
    } else {
        "telegram_stub_delivery_result".to_string()
    };

    let report_content = format!(
        "# Owner Report {}\n\n- contract_id: {}\n- total_revenue: {:.2}\n- visit_count: {}\n- average_check: {:.2}\n- cancellations: {}\n- no_shows: {}\n- flags: {}\n- gate_required: {}\n- delivery: {}\n\n## Draft\n{}\n",
        input.date,
        contract_id,
        metrics.total_revenue,
        metrics.visit_count,
        metrics.average_check,
        metrics.cancellations,
        metrics.no_shows,
        if flags.is_empty() { "none".to_string() } else { flags.join(",") },
        require_human_gate,
        telegram_delivery_result,
        ai_report_draft
    );
    write_text_file(report_path, &report_content)?;

    let audit_json = json!({
        "contract_id": contract_id,
        "run_id": run_id,
        "input_file": input_path,
        "rules_file": rules_path,
        "metrics_result": metrics,
        "anomaly_or_deviation_check_result": {"flags": flags, "require_human_gate": require_human_gate},
        "ai_report_draft": ai_report_draft,
        "human_gate_decision_if_required": if require_human_gate { "pending_or_required" } else { "not_required" },
        "delivery_result": telegram_delivery_result,
        "final_run_status": if require_human_gate { "waiting_human" } else { "completed" }
    });
    write_text_file(audit_path, &serde_json::to_string_pretty(&audit_json).unwrap_or_else(|_| "{}".to_string()))?;

    Ok(GitHubFileE2eResult {
        metrics,
        flags,
        require_human_gate,
        ai_report_draft,
        report_path: report_path.to_string(),
        audit_path: audit_path.to_string(),
        telegram_delivery_result,
    })
}
