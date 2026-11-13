# Диск C: почти заполнен — кэш cargo, сборка, Windows SDK и временные файлы лежат на D:.
# Использование:
#   .\dev.ps1          — запуск окна Kapla в режиме разработки
#   .\dev.ps1 build    — сборка .exe и установщика
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$TauriArgs
)
if (-not $TauriArgs -or $TauriArgs.Count -eq 0) { $TauriArgs = @("dev") }

$root = "D:\kapla-build"
New-Item -ItemType Directory -Force "$root\cargo", "$root\target", "$root\tmp" | Out-Null
$env:CARGO_HOME = "$root\cargo"
$env:CARGO_TARGET_DIR = "$root\target"
$env:TEMP = "$root\tmp"
$env:TMP = "$root\tmp"
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# Подключаем MSVC (link.exe) и библиотеки Windows SDK
if (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        $vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
        if (Test-Path $vcvars) {
            cmd /c "`"$vcvars`" >nul && set" | ForEach-Object {
                if ($_ -match "^([^=]+)=(.*)$") {
                    [System.Environment]::SetEnvironmentVariable($matches[1], $matches[2], "Process")
                }
            }
        }
    }
}

$sdk = "$root\winsdk\sdk"
$crt = "$root\winsdk\crt"
if (Test-Path "$sdk\lib\um\x64") {
    $env:LIB = "$sdk\lib\um\x64;$sdk\lib\ucrt\x64;$crt\lib\x64;$($env:LIB)"
    $env:INCLUDE = "$sdk\include\um;$sdk\include\ucrt;$sdk\include\shared;$crt\include;$($env:INCLUDE)"
}
$rcBin = Get-ChildItem "$root\sdk-tools\bin\*\x64\rc.exe" -ErrorAction SilentlyContinue | Select -First 1 -ExpandProperty DirectoryName
if ($rcBin) {
    $env:PATH = "$rcBin;$($env:PATH)"
}

Set-Location $PSScriptRoot
npx tauri @TauriArgs
