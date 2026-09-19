#Requires -RunAsAdministrator
param([string]$Package = '', [switch]$Uninstall)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $Package) { $Package = Join-Path $repo 'target/tyu-microphone' }
$Package = (Resolve-Path -LiteralPath $Package).Path
$installer = Join-Path $Package 'TyuMicInstall.exe'
if (-not (Test-Path -LiteralPath $installer)) { throw 'Compila primero tools/build-tyu-microphone.ps1.' }
if ($Uninstall) {
    & $installer --uninstall
    if ($LASTEXITCODE -ne 0) { throw "No se pudo retirar el dispositivo TYU: $LASTEXITCODE" }
    exit
}
$inf = Join-Path $Package 'TyuMicrophone.inf'
$sys = Join-Path $Package 'TyuMicrophone.sys'
$cat = Join-Path $Package 'TyuMicrophone.cat'
foreach ($file in @($inf,$sys,$cat)) { if (-not (Test-Path -LiteralPath $file)) { throw "Falta $file" } }
$signTool = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin/10.0.26100.0/x64/signtool.exe'
if (-not (Test-Path -LiteralPath $signTool)) { throw 'Falta SignTool del Windows SDK para verificar la firma de kernel.' }
# Verification uses the kernel signing policy and checks both package members against the signed catalog.
# Never enable test signing, disable Secure Boot, or import development certificates on the user's PC.
& $signTool verify /kp /v /c $cat $sys
if ($LASTEXITCODE -ne 0) { throw 'Windows requiere un paquete TYU firmado por Microsoft. El paquete local aún no está firmado; no se ha instalado ni se han cambiado opciones de seguridad.' }
& $signTool verify /kp /v /c $cat $inf
if ($LASTEXITCODE -ne 0) { throw 'La firma del INF no coincide con el catálogo. No se instalará.' }
& $installer $inf
if ($LASTEXITCODE -eq 3010) { Write-Host 'TYU Microphone instalado. Windows solicita reiniciar para terminar.' }
elseif ($LASTEXITCODE -ne 0) { throw "No se pudo instalar TYU Microphone: $LASTEXITCODE" }
else { Write-Host 'TYU Microphone instalado. Selecciónalo como entrada de audio y activa Micrófono en Bypass.' }
