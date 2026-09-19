# Run once as administrator after build-tyu-camera.ps1. No external application is installed.
param([switch]$Uninstall)
#Requires -RunAsAdministrator
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$destination = Join-Path $env:ProgramFiles 'TYU/Camera'
if ($Uninstall) {
    $registered = Get-Item 'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Classes\CLSID\{9E125CB7-A9AA-4A82-9E12-A2DF17F43A51}\InprocServer32' -ErrorAction SilentlyContinue
    $dll = if ($registered) { $registered.GetValue('') } else { Join-Path $destination 'TyuCameraSource.dll' }
    if (Test-Path -LiteralPath $dll) {
        $proc = Start-Process -FilePath "$env:windir/System32/regsvr32.exe" -ArgumentList @('/u','/s',('"'+$dll+'"')) -WindowStyle Hidden -Wait -PassThru
        if ($proc.ExitCode -ne 0) { throw 'No se pudo desregistrar la cámara TYU' }
    }
    Write-Host 'Cámara TYU desregistrada. Cierra las aplicaciones de cámara antes de retirar sus archivos.'
    exit
}
$source = Join-Path $repo 'target/tyu-camera'
foreach ($name in @('TyuCameraSource.dll','TyuCameraHost.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $source $name))) { throw 'Ejecuta primero tools/build-tyu-camera.ps1' }
}
New-Item -ItemType Directory -Path $destination -Force | Out-Null
# Frame Server may still have a previous source DLL loaded; register a versioned path for updates.
$version = (Get-FileHash -LiteralPath (Join-Path $source 'TyuCameraSource.dll') -Algorithm SHA256).Hash.Substring(0,16)
$component = Join-Path $destination $version
New-Item -ItemType Directory -Path $component -Force | Out-Null
$dll = Join-Path $component 'TyuCameraSource.dll'
if (-not (Test-Path -LiteralPath $dll)) { Copy-Item -LiteralPath (Join-Path $source 'TyuCameraSource.dll') -Destination $dll }
Copy-Item -LiteralPath (Join-Path $source 'TyuCameraHost.exe') -Destination $destination -Force
$proc = Start-Process -FilePath "$env:windir/System32/regsvr32.exe" -ArgumentList @('/s',('"'+$dll+'"')) -WindowStyle Hidden -Wait -PassThru
if ($proc.ExitCode -ne 0) { throw "Registro fallido: $($proc.ExitCode)" }
Write-Host 'TYU Camera instalada. Activa Cámara en Bypass para publicarla en Windows.'
