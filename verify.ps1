param([switch]$Offline)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    # Use the actual toolchain Cargo: the installed front-end may redirect targets.
    $probeCargo = (& rustup which cargo).Trim()
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $probeCargo)) {
        throw 'Cannot locate the active rustup toolchain Cargo.'
    }
    $probeTarget = Join-Path $PSScriptRoot 'target'
    $probeResults = Join-Path $PSScriptRoot 'results'
    New-Item -ItemType Directory -Force -Path $probeResults | Out-Null
    $common = @('--workspace', '--locked', '--target-dir', $probeTarget)
    if ($Offline) { $common += '--offline' }

    function Invoke-ProbeStep {
        param([string]$Name, [string[]]$CargoArguments)
        $log = Join-Path $probeResults ($Name + '.txt')
        & $probeCargo @CargoArguments 2>&1 | Tee-Object -FilePath $log
        if ($LASTEXITCODE -ne 0) { throw "$Name failed; see $log" }
    }

    @(
        'Verification time: ' + (Get-Date -Format o)
        'Cargo executable: ' + $probeCargo
        'Target directory: ' + $probeTarget
        (& rustc --version)
        (& $probeCargo --version)
        'Offline: ' + $Offline
    ) | Set-Content -LiteralPath (Join-Path $probeResults 'environment.txt')

    Invoke-ProbeStep -Name 'format' -CargoArguments @('fmt', '--all', '--', '--check')
    Invoke-ProbeStep -Name 'tests' -CargoArguments (@('test') + $common)
    Invoke-ProbeStep -Name 'clippy' -CargoArguments (@('clippy', '--all-targets') + $common + @('--', '-D', 'warnings'))
    $wasmArguments = @('check', '-p', 'probe-web', '--target', 'wasm32-unknown-unknown', '--locked', '--target-dir', $probeTarget)
    if ($Offline) { $wasmArguments += '--offline' }
    Invoke-ProbeStep -Name 'web-wasm' -CargoArguments $wasmArguments
    $runArguments = @('run', '--locked', '--target-dir', $probeTarget, '-p', 'probe-demo')
    if ($Offline) { $runArguments += '--offline' }
    Invoke-ProbeStep -Name 'demo' -CargoArguments $runArguments
    Invoke-ProbeStep -Name 'patterns-demo' -CargoArguments ($runArguments + @('--example', 'patterns'))
    $diagnosticsArguments = @('test', '-p', 'probe-ui', '--test', 'diagnostics', '--locked', '--target-dir', $probeTarget)
    if ($Offline) { $diagnosticsArguments += '--offline' }
    Invoke-ProbeStep -Name 'diagnostics' -CargoArguments ($diagnosticsArguments + @('--', '--nocapture'))

    $files = @('Cargo.toml', 'Cargo.lock', '.gitignore', 'verify.ps1', 'README.md', 'MECHANISM.md')
    $files += @(Get-ChildItem -LiteralPath '.github' -Recurse -File |
        ForEach-Object { [IO.Path]::GetRelativePath($PSScriptRoot, $_.FullName) })
    $generatedWeb = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'web/pkg'))
    $files += @(Get-ChildItem -LiteralPath 'macros','runtime','demo','web' -Recurse -File |
        Where-Object { -not $_.FullName.StartsWith($generatedWeb + [IO.Path]::DirectorySeparatorChar) } |
        ForEach-Object { [IO.Path]::GetRelativePath($PSScriptRoot, $_.FullName) })
    $manifest = foreach ($file in $files) {
        [pscustomobject]@{
            path = $file.Replace('\', '/')
            sha256 = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    $manifest | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $probeResults 'source-manifest.json')
    Write-Output 'PASS: formatting, runtime/compile-fail/diagnostic tests, Clippy, and both scripted demos.'
}
finally {
    Pop-Location
}
