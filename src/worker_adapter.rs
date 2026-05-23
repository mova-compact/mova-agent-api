//! Cloudflare Worker deployment adapter for MOVA Agent API V0.
//!
//! Provider-specific shell only. Core request/policy/execution/connectors/
//! observation/evidence/storage modules remain provider-agnostic.

use tower::Service;
use worker::{event, Context, Env, HttpRequest, Result};

#[event(fetch)]
async fn fetch(
    req: HttpRequest,
    _env: Env,
    _ctx: Context,
) -> Result<http::Response<axum::body::Body>> {
    // Reuse the existing HTTP adapter surface unchanged.
    let response = crate::http::router().call(req).await.map_err(|never| match never {})?;
    Ok(response)
}
