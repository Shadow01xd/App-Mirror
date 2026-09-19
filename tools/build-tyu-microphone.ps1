param([string]$WdkRoot = '', [switch]$RestoreWdk, [switch]$Analyze)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$version = '10.0.26100.0'
if (-not $WdkRoot) { $WdkRoot = Join-Path $repo 'target/tyu-wdk/c' }
if ($RestoreWdk -and -not (Test-Path -LiteralPath (Join-Path $WdkRoot "Include/$version/km/portcls.h"))) {
    $package = Join-Path $repo 'target/tyu-wdk.zip'
    New-Item -ItemType Directory -Path (Join-Path $repo 'target') -Force | Out-Null
    Invoke-WebRequest 'https://api.nuget.org/v3-flatcontainer/microsoft.windows.wdk.x64/10.0.26100.6584/microsoft.windows.wdk.x64.10.0.26100.6584.nupkg' -OutFile $package -UseBasicParsing
    Expand-Archive -LiteralPath $package -DestinationPath (Join-Path $repo 'target/tyu-wdk') -Force
}
if (-not (Test-Path -LiteralPath (Join-Path $WdkRoot "Include/$version/km/portcls.h"))) { throw 'Instala el WDK 26100 o ejecuta con -RestoreWdk para descargar el paquete oficial de Microsoft.' }
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'Faltan las herramientas C++ de Visual Studio.' }
& (Join-Path $vs 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
$sdk = Join-Path ${env:ProgramFiles(x86)} "Windows Kits/10/Include/$version"
$out = Join-Path $repo 'target/tyu-microphone'
New-Item -ItemType Directory -Path $out -Force | Out-Null
$native = Join-Path $repo 'desktop/native/microphone'
Push-Location $out
try {
    & cl.exe /nologo /std:c++17 /EHsc /W4 /WX /MT /Od (Join-Path $native 'ring_test.cpp') /Fe:TyuMicRingTest.exe
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo compilar la prueba PCM.' }
    & './TyuMicRingTest.exe'
    if ($LASTEXITCODE -ne 0) { throw 'Falló la prueba PCM.' }
    & cl.exe /nologo /std:c++17 /EHsc /W4 /WX /MT /O2 (Join-Path $native 'install.cpp') /Fe:TyuMicInstall.exe /link setupapi.lib newdev.lib uuid.lib '/MANIFESTUAC:level="requireAdministrator" uiAccess="false"'
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo compilar el instalador TYU.' }
    & cl.exe /nologo /std:c++17 /EHsc /W4 /WX /MT /O2 (Join-Path $native 'probe.cpp') /Fe:TyuMicProbe.exe /link ole32.lib uuid.lib
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo compilar la prueba WASAPI.' }
    # WDK declarations are external headers; keep /W4 /WX and analysis enabled for all TYU code.
    $includes = @('/external:W0', "/external:I$WdkRoot/Include/$version/km", "/external:I$WdkRoot/Include/$version/shared", "/external:I$sdk/shared", "/external:I$sdk/ucrt")
    [string[]]$analysisOptions = if ($Analyze) { @('/analyze','/analyze:external-') } else { @() }
    & cl.exe /nologo /c /std:c++17 /kernel /GR- /Zl /W4 /WX /O2 /GS /guard:cf /D_AMD64_ /DAMD64 /D_WIN64 /D_WIN32_WINNT=0x0A00 /DNTDDI_VERSION=0x0A000008 @includes @analysisOptions (Join-Path $native 'driver.cpp') /Fo:TyuMicrophone.obj
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo compilar el controlador TYU.' }
    $lib = Join-Path $WdkRoot "Lib/$version/km/x64"
    & link.exe /nologo /driver /subsystem:native,10.00 /entry:GsDriverEntry /nodefaultlib /machine:x64 /integritycheck /dynamicbase /nxcompat /guard:cf /out:TyuMicrophone.sys TyuMicrophone.obj "/libpath:$lib" ntoskrnl.lib hal.lib portcls.lib ks.lib ksguid.lib wdmsec.lib BufferOverflowFastFailK.lib
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo enlazar el controlador TYU.' }
    Copy-Item -LiteralPath (Join-Path $native 'TyuMicrophone.inf') -Destination $out -Force
    $infverif = Join-Path $WdkRoot "tools/$version/x64/InfVerif.exe"
    & $infverif /w (Join-Path $out 'TyuMicrophone.inf')
    if ($LASTEXITCODE -ne 0) { throw 'Falló la validación INF.' }
    $inf2cat = Join-Path $WdkRoot "bin/$version/x86/Inf2Cat.exe"
    & $inf2cat "/driver:$out" /os:10_X64 /uselocaltime
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo crear el catálogo del controlador.' }
    Write-Host "Controlador propio compilado en $out. Todavía necesita firma de Microsoft y validación en Windows."
} finally { Pop-Location }
