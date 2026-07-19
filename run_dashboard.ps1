# BPO Dashboard - Lancement pour Windows (PowerShell)
# Equivalent de run_dashboard.sh

$ErrorActionPreference = "Stop"

$RepoDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$DataDir = Join-Path $RepoDir "data"
$ExampleCfg = Join-Path $RepoDir "config.example.json"
$RealCfg = Join-Path $DataDir "config.json"
$Executable = Join-Path $RepoDir "target\release\bpo-dashboard.exe"
$Url = "http://localhost:8090"

# 1. Cree data/ si manquant
if (-not (Test-Path $DataDir)) {
    New-Item -ItemType Directory -Path $DataDir | Out-Null
}

# 2. Copie le template si manquant
if (-not (Test-Path $RealCfg)) {
    Copy-Item $ExampleCfg $RealCfg
}

# 3. Demande les credentials si placeholders encore presents
$content = Get-Content $RealCfg -Raw
if ($content -match "VOTRE_CLIENT_ID" -or $content -match "VOTRE_CLIENT_SECRET") {
    Write-Host "Credentials a configurer." -ForegroundColor Yellow
    $clientId = Read-Host "Client ID"
    $clientSecret = Read-Host "Client Secret"

    $content = $content -replace '"client_id": "[^"]*"', "`"client_id`": `"$clientId`""
    $content = $content -replace '"client_secret": "[^"]*"', "`"client_secret`": `"$clientSecret`""
    Set-Content -Path $RealCfg -Value $content

    Write-Host "Credentials configures." -ForegroundColor Green
} else {
    Write-Host "Credentials deja presents dans $RealCfg." -ForegroundColor Green
}

# 4. Compile si besoin
if (-not (Test-Path $Executable)) {
    Set-Location $RepoDir
    cargo build --release
}

# 5. Lance
Write-Host "Dashboard sur $Url" -ForegroundColor Cyan
$process = Start-Process -FilePath $Executable -PassThru -NoNewWindow

# Ouvre le navigateur
Start-Process $Url

# Attend que le process se termine ou Ctrl+C
try {
    $process.WaitForExit()
} catch {
    if (-not $process.HasExited) {
        $process.Kill()
    }
}