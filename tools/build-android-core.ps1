param([string]$Abis = 'arm64-v8a,x86_64')
$ErrorActionPreference = 'Stop'
$tyuRepo = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$tyuSdk = $env:ANDROID_HOME
if (-not $tyuSdk) { $tyuSdk = Join-Path $env:LOCALAPPDATA 'Android\Sdk' }
$tyuNdk = $env:ANDROID_NDK_HOME
if (-not $tyuNdk) {
    $tyuNdk = (Get-ChildItem -LiteralPath (Join-Path $tyuSdk 'ndk') -Directory | Sort-Object LastWriteTime -Descending | Select-Object -First 1).FullName
}
if (-not $tyuNdk) { throw 'Install Android NDK or set ANDROID_NDK_HOME.' }
$tyuBin = Join-Path $tyuNdk 'toolchains\llvm\prebuilt\windows-x86_64\bin'
$tyuOutput = Join-Path $tyuRepo 'Android\app\build\generated\tyuJniLibs'
Push-Location $tyuRepo
try {
    foreach ($tyuAbi in $Abis.Split(',')) {
        switch ($tyuAbi) {
            'arm64-v8a' { $tyuTarget = 'aarch64-linux-android'; $tyuCompiler = 'aarch64-linux-android26-clang.cmd' }
            'x86_64' { $tyuTarget = 'x86_64-linux-android'; $tyuCompiler = 'x86_64-linux-android26-clang.cmd' }
            default { throw "Unsupported ABI: $tyuAbi" }
        }
        $tyuEnvTarget = $tyuTarget.Replace('-', '_')
        $tyuLinkerName = 'CARGO_TARGET_' + $tyuEnvTarget.ToUpperInvariant() + '_LINKER'
        [Environment]::SetEnvironmentVariable($tyuLinkerName, (Join-Path $tyuBin $tyuCompiler), 'Process')
        [Environment]::SetEnvironmentVariable(('CC_' + $tyuEnvTarget), (Join-Path $tyuBin $tyuCompiler), 'Process')
        [Environment]::SetEnvironmentVariable(('AR_' + $tyuEnvTarget), (Join-Path $tyuBin 'llvm-ar.exe'), 'Process')
        cargo build -p tyu-ffi --release --target $tyuTarget -j 2
        if ($LASTEXITCODE -ne 0) { throw "Rust build failed for $tyuAbi" }
        $tyuAbiOutput = Join-Path $tyuOutput $tyuAbi
        New-Item -ItemType Directory -Path $tyuAbiOutput -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $tyuRepo "target\$tyuTarget\release\libtyu_ffi.so") -Destination (Join-Path $tyuAbiOutput 'libtyu_ffi.so')
    }
} finally { Pop-Location }
