param(
  [string]$BaseUrl = "https://mova-agent-api-v0.s-myasoedov81.workers.dev",
  [string]$WebhookUrl = "",
  [string]$EndpointRef = "webhook_site_test",
  [string]$IdempotencyKey = ""
)

$ErrorActionPreference = "Stop"

$requestPath = "D:/Projects_MOVA/mova-agent-api/examples/agent_request_http_generic_webhook.json"
$payload = Get-Content -Raw $requestPath | ConvertFrom-Json
$rid = "req_smoke_{0}" -f [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
$payload.request_id = $rid
$payload.action.action_id = "act_smoke_01"
$payload.action.trace_ref = "trace:$rid"
$payload.correlation = @{ correlation_id = "corr:$rid"; trace_id = "trace:$rid" }

if ($WebhookUrl -ne "") {
  $authContext = @{
    mode = "placeholder"
    scopes = @("actions.run")
    verified = $false
    source = "smoke_script"
  }
  if ($payload.PSObject.Properties.Match("auth_context").Count -gt 0) {
    $payload.auth_context = $authContext
  } else {
    $payload | Add-Member -NotePropertyName "auth_context" -NotePropertyValue $authContext
  }
  $payload.action.action_type = "webhook_notify"
  $payload.action.connector_context = @{
    connector_id = "connector.http.generic.v1"
    side_effect_intent = "external_network"
    endpoint_ref = $EndpointRef
    method = "POST"
    headers = @{
      "x-mova-smoke" = "public"
    }
  }
  $payload.action.input_payload = @{
    message = "MOVA universal HTTP smoke"
    target_url_hint = $WebhookUrl
  }
}

$body = $payload | ConvertTo-Json -Depth 20 -Compress
$headers = @{}
if ($IdempotencyKey -ne "") {
  $headers["Idempotency-Key"] = $IdempotencyKey
}

$health = Invoke-RestMethod -Method Get -Uri "$BaseUrl/health"
$ready = Invoke-RestMethod -Method Get -Uri "$BaseUrl/ready"
$cap = Invoke-RestMethod -Method Get -Uri "$BaseUrl/capabilities"
$val = Invoke-RestMethod -Method Post -Uri "$BaseUrl/actions/validate" -ContentType "application/json" -Body $body
$run = Invoke-RestMethod -Method Post -Uri "$BaseUrl/actions/run" -Headers $headers -ContentType "application/json" -Body $body
$runId = $run.run_id
$status = Invoke-RestMethod -Method Get -Uri "$BaseUrl/runs/$runId"
$evidence = Invoke-RestMethod -Method Get -Uri "$BaseUrl/runs/$runId/evidence"

[ordered]@{
  health = $health
  ready = $ready
  capabilities = $cap
  validate = $val
  run = $run
  run_status = $status
  evidence = $evidence
} | ConvertTo-Json -Depth 20
