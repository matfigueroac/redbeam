[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $Iscc,
    [Parameter(Mandatory)][string] $CargoAbout,
    [string] $OutputDirectory,
    [string] $ExpectedTag
)

$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
if (!$OutputDirectory) { $OutputDirectory = Join-Path $repository 'dist' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$Iscc = (Get-Item -LiteralPath $Iscc).FullName
$CargoAbout = (Get-Item -LiteralPath $CargoAbout).FullName
$versionTag = 'x86_64-pc-windows-gnullvm'

function Invoke-Checked([string] $Program, [string[]] $Arguments) {
    $rtk = Get-Command rtk -ErrorAction SilentlyContinue
    if ($rtk) { & $rtk.Source $Program @Arguments } else { & $Program @Arguments }
    if ($LASTEXITCODE -ne 0) { throw "$Program terminó con código $LASTEXITCODE." }
}

Push-Location -LiteralPath $repository
try {
    $target = Invoke-Checked 'rustc' @('-vV') | Select-String '^host: (.+)$'
    if ($target.Matches.Groups[1].Value -ne $versionTag) {
        throw "Activa la toolchain Rust $versionTag antes de generar el instalador."
    }
    $metadata = Invoke-Checked 'cargo' @('metadata', '--locked', '--no-deps', '--format-version=1') | ConvertFrom-Json
    $version = ($metadata.packages | Where-Object name -EQ 'redbim').version
    if ($version -notmatch '^\d+\.\d+\.\d+$') { throw "Versión de RedBIM no válida: $version" }
    if ($ExpectedTag -and $ExpectedTag -cne "v$version") {
        throw "La etiqueta $ExpectedTag no coincide con Cargo.toml (v$version)."
    }

    Invoke-Checked 'cargo' @('build', '--locked', '--release', '--bin', 'redbim')
    $binary = Join-Path $metadata.target_directory 'release/redbim.exe'
    if ((Get-Item -LiteralPath $binary).VersionInfo.ProductVersion -ne $version) {
        throw 'La versión del ejecutable no coincide con Cargo.toml.'
    }
    $imports = Invoke-Checked 'llvm-objdump' @('-p', $binary) | Select-String 'DLL Name:'
    if (!$imports -or $imports -match '(?i)libwinpthread|libgcc|libc\+\+|libstdc\+\+') {
        throw 'No se pudieron verificar todas las dependencias de RedBIM.'
    }

    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $notices = Join-Path $OutputDirectory 'Licencias-de-terceros.html'
    Invoke-Checked $CargoAbout @('generate', '--locked', '--target', $versionTag, '--fail', '--output-file', $notices, 'installer/licenses.hbs')
    if ((Get-Item -LiteralPath $notices).Length -eq 0) { throw 'El informe de licencias está vacío.' }
    $llvmRoot = Split-Path -Parent (Split-Path -Parent (Get-Command llvm-objdump).Source)
    $llvmLicense = Join-Path $llvmRoot 'LICENSE.TXT'
    $runtime = Join-Path $llvmRoot 'x86_64-w64-mingw32/bin/libunwind.dll'
    if (!(Test-Path -LiteralPath $llvmLicense) -or !(Test-Path -LiteralPath $runtime)) {
        throw 'No se encontró el runtime x64 de LLVM-MinGW o su licencia.'
    }

    Invoke-Checked $Iscc @('/Qp', "/O$OutputDirectory", "/DAppVersion=$version", "/DAppBinary=$binary", "/DThirdPartyNotices=$notices", "/DLlvmLicense=$llvmLicense", "/DLlvmRuntime=$runtime", 'installer/RedBIM.iss')
    $installer = Join-Path $OutputDirectory 'RedBIM-Setup-x64.exe'
    $hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -LiteralPath (Join-Path $OutputDirectory 'SHA256SUMS.txt') -Value "$hash  RedBIM-Setup-x64.exe`n" -Encoding ascii -NoNewline
    Write-Host "RedBIM ${version}: $installer"
} finally {
    Pop-Location
}
