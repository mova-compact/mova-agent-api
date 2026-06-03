param(
  [string]$BaseUrl = "http://127.0.0.1:8787",
  [string]$ApiKey = "mova-dev-key"
)

$ErrorActionPreference = "Stop"

function Invoke-JsonRequest {
  param(
    [string]$Method,
    [string]$Url,
    [object]$Body = $null,
    [int]$ExpectedStatus = 200
  )

  $params = @{
    Method = $Method
    Uri = $Url
    ContentType = "application/json"
    Headers = @{ "x-mova-api-key" = $ApiKey }
  }
  if ($null -ne $Body) {
    $params.Body = ($Body | ConvertTo-Json -Depth 10)
  }

  try {
    $response = Invoke-WebRequest @params
  } catch {
    if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -ne $ExpectedStatus) {
      throw
    }
    $response = $_.Exception.Response
  }

  if ([int]$response.StatusCode -ne $ExpectedStatus) {
    throw "Unexpected status for $Method ${Url}: got $([int]$response.StatusCode), expected $ExpectedStatus"
  }

  if ($response.Content) {
    return $response.Content | ConvertFrom-Json
  }
  return $null
}

$null = Invoke-JsonRequest -Method GET -Url "$BaseUrl/health"
$null = Invoke-JsonRequest -Method GET -Url "$BaseUrl/ready"
$null = Invoke-JsonRequest -Method GET -Url "$BaseUrl/capabilities"

$startBody = Get-Content -Raw (Join-Path $PSScriptRoot "..\examples\contract_run_start_minimal.json") | ConvertFrom-Json
$start = Invoke-JsonRequest -Method POST -Url "$BaseUrl/contracts/daily_owner_report_v0/runs" -Body $startBody -ExpectedStatus 202
$runId = $start.run_id

$null = Invoke-JsonRequest -Method GET -Url "$BaseUrl/contract-runs/$runId"
$next = Invoke-JsonRequest -Method GET -Url "$BaseUrl/contract-runs/$runId/next"
$stepId = $next.step.step_id

$executeBody = @{
  operation_id = $next.operation_admission.operation_id
  input_payload = @{ message = "contract-run smoke" }
  correlation = @{
    trace_id = "trace_contract_001"
    correlation_id = "corr_contract_001"
  }
}
$null = Invoke-JsonRequest -Method POST -Url "$BaseUrl/contract-runs/$runId/steps/$stepId/execute" -Body $executeBody -ExpectedStatus 202

$gate = Invoke-JsonRequest -Method GET -Url "$BaseUrl/contract-runs/$runId/gates/current"
if ($gate.gate_id) {
  $resolveBody = Get-Content -Raw (Join-Path $PSScriptRoot "..\examples\human_gate_resolve_approve.json") | ConvertFrom-Json
  $null = Invoke-JsonRequest -Method POST -Url "$BaseUrl/contract-runs/$runId/gates/$($gate.gate_id)/resolve" -Body $resolveBody
  $next = Invoke-JsonRequest -Method GET -Url "$BaseUrl/contract-runs/$runId/next"
  $stepId = $next.step.step_id
  $executeBody.operation_id = $next.operation_admission.operation_id
  $null = Invoke-JsonRequest -Method POST -Url "$BaseUrl/contract-runs/$runId/steps/$stepId/execute" -Body $executeBody -ExpectedStatus 202
}

$evidence = Invoke-JsonRequest -Method GET -Url "$BaseUrl/contract-runs/$runId/evidence"

[ordered]@{
  run = $start
  next = $next
  gate = $gate
  evidence = $evidence
} | ConvertTo-Json -Depth 20
