[CmdletBinding(DefaultParameterSetName = 'Installer')]
param(
    [Parameter(Mandatory, ParameterSetName = 'Installer')][string] $Iscc,
    [Parameter(Mandatory, ParameterSetName = 'Payload')][switch] $PayloadOnly,
    [Parameter(Mandatory)][string] $CargoAbout,
    [string] $OutputDirectory,
    [string] $ExpectedTag,
    [Parameter(ParameterSetName = 'Installer')][ValidatePattern('^[0-9a-fA-F]{40}$')][string] $SigningCertificateThumbprint,
    [Parameter(ParameterSetName = 'Installer')][string] $SignTool = 'signtool.exe',
    [Parameter(ParameterSetName = 'Installer')][uri] $TimestampUrl = 'http://timestamp.digicert.com'
)

$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
if (!$OutputDirectory) { $OutputDirectory = Join-Path $repository 'dist' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if (!$PayloadOnly) { $Iscc = (Get-Item -LiteralPath $Iscc).FullName }
$CargoAbout = (Get-Item -LiteralPath $CargoAbout).FullName
$versionTag = 'x86_64-pc-windows-gnullvm'

function Invoke-Checked([string] $Program, [string[]] $Arguments) {
    $rtk = Get-Command rtk -ErrorAction SilentlyContinue
    if ($rtk) { & $rtk.Source $Program @Arguments } else { & $Program @Arguments }
    if ($LASTEXITCODE -ne 0) { throw "$Program terminó con código $LASTEXITCODE." }
}

Push-Location -LiteralPath $repository
try {
    if ($SigningCertificateThumbprint) {
        $cert = Get-Item -LiteralPath "Cert:\CurrentUser\My\$SigningCertificateThumbprint" -ErrorAction SilentlyContinue
        if (!$cert -or !$cert.HasPrivateKey) {
            throw 'El certificado de firma debe estar en el almacén personal de Windows y tener acceso a su clave privada.'
        }
        $SignTool = (Get-Command $SignTool -CommandType Application).Source
    }
    $target = Invoke-Checked 'rustc' @('-vV') | Select-String '^host: (.+)$'
    if ($target.Matches.Groups[1].Value -ne $versionTag) {
        throw "Activa la toolchain Rust $versionTag antes de generar el instalador."
    }
    $metadata = Invoke-Checked 'cargo' @('metadata', '--locked', '--no-deps', '--format-version=1') | ConvertFrom-Json
    $version = ($metadata.packages | Where-Object name -EQ 'redbim').version
    if ($version -notmatch '^\d+\.\d+\.\d+$') { throw "Versión de RedBeam no válida: $version" }
    if ($ExpectedTag -and $ExpectedTag -cne "v$version") {
        throw "La etiqueta $ExpectedTag no coincide con Cargo.toml (v$version)."
    }

    Invoke-Checked 'cargo' @('build', '--locked', '--release', '--bin', 'redbim') | Out-Host
    $binary = Join-Path $metadata.target_directory 'release/redbim.exe'
    if ((Get-Item -LiteralPath $binary).VersionInfo.ProductVersion -ne $version) {
        throw 'La versión del ejecutable no coincide con Cargo.toml.'
    }
    $imports = Invoke-Checked 'llvm-objdump' @('-p', $binary) | Select-String 'DLL Name:'
    if (!$imports -or $imports -match '(?i)libwinpthread|libgcc|libc\+\+|libstdc\+\+') {
        throw 'No se pudieron verificar todas las dependencias de RedBeam.'
    }

    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $notices = Join-Path $OutputDirectory 'Licencias-de-terceros.html'
    Invoke-Checked $CargoAbout @('generate', '--locked', '--target', $versionTag, '--fail', '--output-file', $notices, 'installer/licenses.hbs') | Out-Host
    if ((Get-Item -LiteralPath $notices).Length -eq 0) { throw 'El informe de licencias está vacío.' }
    $llvmRoot = Split-Path -Parent (Split-Path -Parent (Get-Command llvm-objdump).Source)
    $llvmLicense = Join-Path $llvmRoot 'LICENSE.TXT'
    $runtime = Join-Path $llvmRoot 'x86_64-w64-mingw32/bin/libunwind.dll'
    if (!(Test-Path -LiteralPath $llvmLicense) -or !(Test-Path -LiteralPath $runtime)) {
        throw 'No se encontró el runtime x64 de LLVM-MinGW o su licencia.'
    }
    if ($PayloadOnly) {
        return [pscustomobject]@{ Version = $version; Binary = $binary; Runtime = $runtime; LlvmLicense = $llvmLicense; Notices = $notices }
    }

    $installerName = 'RedBeam-Setup-x64'
    $installer = Join-Path $OutputDirectory "$installerName.exe"
    $buildName = $installerName
    if ($SigningCertificateThumbprint) { $buildName = "$installerName-$([guid]::NewGuid().ToString('N'))" }
    $candidate = Join-Path $OutputDirectory "$buildName.exe"
    try {
        Invoke-Checked $Iscc @('/Qp', "/O$OutputDirectory", "/F$buildName", "/DAppVersion=$version", "/DAppBinary=$binary", "/DThirdPartyNotices=$notices", "/DLlvmLicense=$llvmLicense", "/DLlvmRuntime=$runtime", 'installer/RedBeam.iss')
        if ($SigningCertificateThumbprint) {
            Invoke-Checked $SignTool @('sign', '/sha1', $SigningCertificateThumbprint, '/fd', 'SHA256', '/tr', $TimestampUrl.AbsoluteUri, '/td', 'SHA256', $candidate)
            Invoke-Checked $SignTool @('verify', '/pa', '/tw', $candidate)
            $signature = Get-AuthenticodeSignature -LiteralPath $candidate
            if ($signature.Status -ne 'Valid' -or !$signature.TimeStamperCertificate -or $signature.SignerCertificate.Thumbprint -ne $SigningCertificateThumbprint) {
                throw 'La firma del instalador no es válida, no tiene sello de tiempo o corresponde a otro certificado.'
            }
            [IO.File]::Move($candidate, $installer, $true)
        }
    } finally {
        if ($SigningCertificateThumbprint -and (Test-Path -LiteralPath $candidate)) { Remove-Item -LiteralPath $candidate }
    }
    $hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -LiteralPath (Join-Path $OutputDirectory 'SHA256SUMS.txt') -Value "$hash  RedBeam-Setup-x64.exe`n" -Encoding ascii -NoNewline
    Write-Host "RedBeam ${version}: $installer"
} finally {
    Pop-Location
}
