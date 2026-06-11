#!/usr/bin/env pwsh
# BeeBotOS Production Runner (Windows)
# Usage: .\beebotos-run.ps1 [start|stop|restart|status] [gateway|web|beehub|all]

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $ScriptDir

# Ensure data directories exist
$DataDir = Join-Path $ScriptDir "data"
$RunDir = Join-Path $DataDir "run"
$LogDir = Join-Path $DataDir "logs"
New-Item -ItemType Directory -Force -Path $RunDir | Out-Null
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

function Update-ProcessPathFromRegistry {
    $paths = New-Object System.Collections.Generic.List[string]
    foreach ($scope in @(
        "Registry::HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        "Registry::HKEY_CURRENT_USER\Environment"
    )) {
        try {
            $value = (Get-ItemProperty -Path $scope -Name Path -ErrorAction SilentlyContinue).Path
            if (-not [string]::IsNullOrWhiteSpace($value)) {
                foreach ($entry in ($value -split ';')) {
                    $expanded = [Environment]::ExpandEnvironmentVariables($entry.Trim())
                    if (-not [string]::IsNullOrWhiteSpace($expanded)) {
                        $paths.Add($expanded)
                    }
                }
            }
        } catch {}
    }

    foreach ($entry in (($env:Path -split ';') | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })) {
        $paths.Add([Environment]::ExpandEnvironmentVariables($entry.Trim()))
    }

    $seen = @{}
    $merged = New-Object System.Collections.Generic.List[string]
    foreach ($entry in $paths) {
        $key = $entry.TrimEnd('\').ToLowerInvariant()
        if (-not $seen.ContainsKey($key)) {
            $seen[$key] = $true
            $merged.Add($entry)
        }
    }
    $env:Path = ($merged -join ';')
}

Update-ProcessPathFromRegistry

function New-LocalSecret {
    $bytes = New-Object byte[] 32
    $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    try {
        $rng.GetBytes($bytes)
    } finally {
        $rng.Dispose()
    }
    $hex = -join ($bytes | ForEach-Object { $_.ToString("x2") })
    return "bee-jwt-$hex"
}

function Ensure-RuntimeEnvFile {
    $envFile = Join-Path $ScriptDir ".env"
    $hasValidJwtSecret = $false

    if (Test-Path $envFile) {
        foreach ($line in Get-Content $envFile) {
            $trimmed = $line.Trim()
            if ([string]::IsNullOrWhiteSpace($trimmed) -or $trimmed.StartsWith("#")) { continue }
            $parts = $trimmed.Split("=", 2)
            if ($parts.Count -ne 2) { continue }
            $key = $parts[0].Trim()
            $value = $parts[1].Trim().Trim('"').Trim("'")
            if ($key -eq "BEE__JWT__SECRET" -and $value.Length -ge 32) {
                $hasValidJwtSecret = $true
                break
            }
        }
    } else {
        New-Item -ItemType File -Force -Path $envFile | Out-Null
    }

    if (-not $hasValidJwtSecret) {
        Add-Content -Path $envFile -Value "BEE__JWT__SECRET=$(New-LocalSecret)"
    }
}

$Services = @(
    @{ Name = "gateway"; Binary = "beebotos-gateway.exe"; Port = 8000; Desc = "API Gateway" }
    @{ Name = "web";     Binary = "web-server.exe";       Port = 8090; Desc = "Web Frontend Server" }
    @{ Name = "beehub";  Binary = "beehub.exe";           Port = 8080; Desc = "BeeHub Service" }
)

function Import-EnvFile {
    $envFile = Join-Path $ScriptDir ".env"
    if (-not (Test-Path $envFile)) { return }

    foreach ($line in Get-Content $envFile) {
        $trimmed = $line.Trim()
        if ([string]::IsNullOrWhiteSpace($trimmed) -or $trimmed.StartsWith("#")) { continue }
        $parts = $trimmed.Split("=", 2)
        if ($parts.Count -ne 2) { continue }
        $key = $parts[0].Trim()
        $value = $parts[1].Trim().Trim('"').Trim("'")
        if (-not [string]::IsNullOrWhiteSpace($key)) {
            Set-Item -Path "Env:$key" -Value $value
        }
    }
}

Ensure-RuntimeEnvFile
Import-EnvFile

if ([string]::IsNullOrWhiteSpace($env:BEEHUB_PORT)) {
    $env:BEEHUB_PORT = "8080"
}
if ([string]::IsNullOrWhiteSpace($env:BEEHUB_URL)) {
    $env:BEEHUB_URL = "http://localhost:$($env:BEEHUB_PORT)"
}

function Test-IsRunning($name) {
    $pidFile = Join-Path $RunDir "$name.pid"
    if (Test-Path $pidFile) {
        $svcPid = Get-Content $pidFile -Raw
        $svcPid = $svcPid.Trim()
        try {
            $proc = Get-Process -Id $svcPid -ErrorAction SilentlyContinue
            if ($proc) { return $true }
        } catch {}
    }
    return $false
}

function Start-ServiceByName($name) {
    $svc = $Services | Where-Object { $_.Name -eq $name } | Select-Object -First 1
    if (-not $svc) {
        Write-Host "Unknown service: $name" -ForegroundColor Red
        return $false
    }

    $binaryPath = Join-Path $ScriptDir $svc.Binary
    if (-not (Test-Path $binaryPath)) {
        if ($name -eq "beehub") {
            Write-Host "BeeHub binary not found, skipping."
            return $true
        }
        Write-Host "Binary not found: $binaryPath" -ForegroundColor Red
        return $false
    }

    if (Test-IsRunning $name) {
        $svcPid = (Get-Content (Join-Path $RunDir "$name.pid") -Raw).Trim()
        Write-Host "$($svc.Desc) is already running (PID: $svcPid)" -ForegroundColor Yellow
        return $true
    }

    Write-Host "Starting $($svc.Desc) on port $($svc.Port)..."
    $logFile = Join-Path $LogDir "$name.log"
    $errFile = Join-Path $LogDir "$name.err"
    $pidFile = Join-Path $RunDir "$name.pid"
    $procParams = @{
        FilePath               = $binaryPath
        RedirectStandardOutput = $logFile
        RedirectStandardError  = $errFile
        PassThru               = $true
        WorkingDirectory       = $ScriptDir
        WindowStyle            = "Hidden"
    }
    if ($name -eq "web") {
        $webConfigPath = Join-Path $ScriptDir "config\web-server.toml"
        $procParams.ArgumentList = @(
            "--config",
            "`"$webConfigPath`"",
            "--static-path",
            "`"$ScriptDir`""
        )
    }
    $proc = Start-Process @procParams
    $proc.Id | Set-Content $pidFile -NoNewline
    Start-Sleep -Seconds 1
    try {
        $check = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
        if ($check) {
            Write-Host "$($svc.Desc) started (PID: $($proc.Id))" -ForegroundColor Green
            return $true
        }
    } catch {}
    Write-Host "$($svc.Desc) failed to start. Check $logFile and $errFile" -ForegroundColor Red
    Remove-Item $pidFile -Force -ErrorAction SilentlyContinue
    return $false
}

function Stop-ServiceByName($name) {
    $svc = $Services | Where-Object { $_.Name -eq $name } | Select-Object -First 1
    if (-not $svc) {
        Write-Host "Unknown service: $name" -ForegroundColor Red
        return
    }

    $pidFile = Join-Path $RunDir "$name.pid"
    if (-not (Test-IsRunning $name)) {
        Write-Host "$($svc.Desc) is not running" -ForegroundColor Yellow
        Remove-Item $pidFile -Force -ErrorAction SilentlyContinue
        return
    }

    $svcPid = (Get-Content $pidFile -Raw).Trim()
    Write-Host "Stopping $($svc.Desc) (PID: $svcPid)..." -ForegroundColor Cyan
    try {
        Stop-Process -Id $svcPid -Force -ErrorAction Stop
        Write-Host "$($svc.Desc) stopped" -ForegroundColor Green
    } catch {
        Write-Host "Could not stop $($svc.Desc) gracefully: $($_.Exception.Message)" -ForegroundColor Yellow
    }
    Remove-Item $pidFile -Force -ErrorAction SilentlyContinue
}

function Restart-ServiceByName($name) {
    Stop-ServiceByName $name
    Start-Sleep -Seconds 1
    return Start-ServiceByName $name
}

function Show-Status {
    Write-Host "Service Status" -ForegroundColor Cyan
    Write-Host "----------------------------------------" -ForegroundColor Cyan
    Write-Host ("{0,-12} {1,-10} {2,-8} {3}" -f "Service", "Status", "PID", "Port")
    Write-Host "----------------------------------------"
    foreach ($svc in $Services) {
        $pidFile = Join-Path $RunDir "$($svc.Name).pid"
        if (Test-IsRunning $svc.Name) {
            $svcPid = (Get-Content $pidFile -Raw).Trim()
            $line = "{0,-12} {1,-10} {2,-8} {3}" -f $svc.Name, "running", $svcPid, $svc.Port
            Write-Host $line -ForegroundColor Green
        } else {
            $line = "{0,-12} {1,-10} {2,-8} {3}" -f $svc.Name, "stopped", "-", $svc.Port
            Write-Host $line -ForegroundColor Red
        }
    }
}

$action = if ($args.Count -gt 0) { $args[0] } else { "start" }
$target = if ($args.Count -gt 1) { $args[1] } else { "all" }

switch ($action) {
    "start" {
        switch ($target) {
            "gateway" { if (-not (Start-ServiceByName "gateway")) { exit 1 } }
            "web"     { if (-not (Start-ServiceByName "web"))     { exit 1 } }
            "beehub"  { if (-not (Start-ServiceByName "beehub"))  { exit 1 } }
            "all" {
                $ok = $true
                foreach ($svc in $Services) {
                    if (-not (Start-ServiceByName $svc.Name)) { $ok = $false }
                }
                if (-not $ok) { exit 1 }
            }
            default {
                Write-Host "Usage: $($MyInvocation.MyCommand.Name) start [gateway|web|beehub|all]" -ForegroundColor Red
                exit 1
            }
        }
    }
    "stop" {
        switch ($target) {
            "gateway" { Stop-ServiceByName "gateway" }
            "web"     { Stop-ServiceByName "web" }
            "beehub"  { Stop-ServiceByName "beehub" }
            "all" {
                foreach ($svc in $Services) { Stop-ServiceByName $svc.Name }
            }
            default {
                Write-Host "Usage: $($MyInvocation.MyCommand.Name) stop [gateway|web|beehub|all]" -ForegroundColor Red
                exit 1
            }
        }
    }
    "restart" {
        switch ($target) {
            "gateway" { if (-not (Restart-ServiceByName "gateway")) { exit 1 } }
            "web"     { if (-not (Restart-ServiceByName "web"))     { exit 1 } }
            "beehub"  { if (-not (Restart-ServiceByName "beehub"))  { exit 1 } }
            "all" {
                $ok = $true
                foreach ($svc in $Services) {
                    if (-not (Restart-ServiceByName $svc.Name)) { $ok = $false }
                }
                if (-not $ok) { exit 1 }
            }
            default {
                Write-Host "Usage: $($MyInvocation.MyCommand.Name) restart [gateway|web|beehub|all]" -ForegroundColor Red
                exit 1
            }
        }
    }
    "status" { Show-Status }
    default {
        Write-Host "Usage: $($MyInvocation.MyCommand.Name) [start|stop|restart|status] [gateway|web|beehub|all]" -ForegroundColor Red
        exit 1
    }
}
