param([string]$Configuration = 'Release')
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'Instala las herramientas C++ de Visual Studio y Windows SDK 10.0.26100 o posterior.' }
$dev = Join-Path $vs 'Common7/Tools/Launch-VsDevShell.ps1'
& $dev -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
$out = Join-Path $repo 'target/tyu-camera'
New-Item -ItemType Directory -Path $out -Force | Out-Null
Push-Location $out
try {
    $source = Join-Path $repo 'desktop/native/camera'
    & cl.exe /nologo /std:c++17 /EHsc /O2 /MT /DUNICODE /D_UNICODE /D_WIN32_WINNT=0x0A00 /LD (Join-Path $source 'source.cpp') /Fe:TyuCameraSource.dll /link ('/DEF:'+(Join-Path $source 'source.def')) mfplat.lib mf.lib mfuuid.lib advapi32.lib ole32.lib propsys.lib shlwapi.lib
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo compilar TyuCameraSource.dll' }
    & cl.exe /nologo /std:c++17 /EHsc /O2 /MT /DUNICODE /D_UNICODE /D_WIN32_WINNT=0x0A00 (Join-Path $source 'host.cpp') /Fe:TyuCameraHost.exe /link mfplat.lib mf.lib mfuuid.lib mfreadwrite.lib mfsensorgroup.lib advapi32.lib ole32.lib
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo compilar TyuCameraHost.exe' }
    Write-Host "Cámara propia compilada en $out"
} finally { Pop-Location }
