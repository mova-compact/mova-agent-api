param(
  [string]$BaseUrl = "https://mova-agent-api-v0.s-myasoedov81.workers.dev",
  [string]$ContractId = "provider_connector_owner_report_v0",
  [string]$Text = "MOVA provider connector smoke"
)

$ErrorActionPreference = "Stop"

function Invoke-JsonRequest {
  param(
    [string]$Method,
    [string]$Url,
    [object]$Body = $null,
    [int[]]$ExpectedStatus = @(200)
  )

  $tempFile = $null
  $curlArgs = @(
    "-sS",
    "-X", $Method,
    "-H", "content-type: application/json",
    "-w", "`nSTATUS:%{http_code}"
  )
  if ($null -ne $Body) {
    $tempFile = [System.IO.Path]::GetTempFileName()
    ($Body | ConvertTo-Json -Depth 20) | Set-Content -LiteralPath $tempFile -NoNewline
    $curlArgs += @("--data-binary", "@$tempFile")
  }
  $curlArgs += $Url

  try {
    $raw = & curl.exe @curlArgs
  } finally {
    if ($null -ne $tempFile -and (Test-Path -LiteralPath $tempFile)) {
      Remove-Item -LiteralPath $tempFile -Force
    }
  }

  $lines = $raw -split "`r?`n"
  $statusLine = $lines[-1]
  if ([string]::IsNullOrWhiteSpace($statusLine) -and $lines.Length -ge 2) {
    $statusLine = $lines[-2]
    $lines = $lines[0..($lines.Length - 2)]
  }
  $statusMarker = [regex]::Match($statusLine, "^STATUS:(\d{3})$")
  if (-not $statusMarker.Success) {
    throw "Unable to parse HTTP status for $Method ${Url}"
  }
  $statusCode = [int]$statusMarker.Groups[1].Value
  if ($ExpectedStatus -notcontains $statusCode) {
    throw "Unexpected status for $Method ${Url}: got $statusCode, expected one of $($ExpectedStatus -join ', ')"
  }

  if ($lines.Length -gt 1) {
    $bodyText = ($lines[0..($lines.Length - 2)] -join "`n").Trim()
  } else {
    $bodyText = ""
  }
  $payload = $null
  if ($bodyText) {
    $payload = $bodyText | ConvertFrom-Json
  }

  [pscustomobject]@{
    StatusCode = $statusCode
    Json = $payload
  }
}

$startBody = Get-Content -Raw (Join-Path $PSScriptRoot "..\examples\contract_run_start_minimal.json") | ConvertFrom-Json
$start = Invoke-JsonRequest -Method POST -Url "$BaseUrl/contracts/$ContractId/runs" -Body $startBody -ExpectedStatus @(202)
$runId = $start.Json.run_id

$status = Invoke-JsonRequest -Method GET -Url "$BaseUrl/contract-runs/$runId" -ExpectedStatus @(200)
$next = Invoke-JsonRequest -Method GET -Url "$BaseUrl/contract-runs/$runId/next" -ExpectedStatus @(200)

if ($next.Json.operation_admission.allowed_connector_id -ne "provider.connector.v1") {
  throw "Unexpected connector_id in operation admission: $($next.Json.operation_admission.allowed_connector_id)"
}

$stepId = $next.Json.step.step_id
$executeBody = @{
  operation_id = $next.Json.operation_admission.operation_id
  input_payload = @{
    text = $Text
  }
  correlation = @{
    trace_id = "trace_contract_001"
    correlation_id = "corr_contract_001"
  }
}

$execute = Invoke-JsonRequest -Method POST -Url "$BaseUrl/contract-runs/$runId/steps/$stepId/execute" -Body $executeBody -ExpectedStatus @(202, 503)
$evidence = Invoke-JsonRequest -Method GET -Url "$BaseUrl/contract-runs/$runId/evidence" -ExpectedStatus @(200)

$verdict = "PASS"
$notes = @()

if ($execute.StatusCode -eq 503) {
  $errorCode = $execute.Json.error.code
  $connectorDetail = @($execute.Json.error.details) | Where-Object { $_ -like "connector_code:*" } | Select-Object -First 1
  $connectorCode = if ($connectorDetail) { ($connectorDetail -replace "^connector_code:\s*", "") } else { $null }
  if ($errorCode -ne "connector_secret_missing" -and $connectorCode -ne "connector_secret_missing") {
    throw "Unexpected execute error code: $errorCode"
  }
  $verdict = "PASS_WITH_WARNINGS"
  $notes += "Telegram secrets missing; provider proxy is wired but live send is not configured."
} elseif ($evidence.Json.status -ne "completed") {
  throw "Expected completed evidence status, got $($evidence.Json.status)"
}

[ordered]@{
  verdict = $verdict
  run = $start.Json
  status = $status.Json
  next = $next.Json
  execute = $execute.Json
  evidence = $evidence.Json
  notes = $notes
} | ConvertTo-Json -Depth 20
