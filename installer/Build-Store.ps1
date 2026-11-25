[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $CargoAbout,
    [Parameter(Mandatory)][ValidatePattern('^[a-zA-Z0-9.-]{3,50}$')][string] $PackageName,
    [Parameter(Mandatory)][string] $Publisher,
    [Parameter(Mandatory)][string] $PublisherDisplayName,
    [string] $MakeAppx = 'makeappx.exe',
    [string] $OutputDirectory
)

$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
if (!$OutputDirectory) { $OutputDirectory = Join-Path $repository 'dist/store' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$MakeAppx = (Get-Command $MakeAppx -CommandType Application).Source
$stage = Join-Path $OutputDirectory "msix-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $stage -Force | Out-Null

$manifestPath = Join-Path $stage 'AppxManifest.xml'
$mappingPath = Join-Path $stage 'package-map.txt'
$candidate = Join-Path $stage 'RedBeam-Store-x64.msix'
$logoSizes = [ordered]@{ 'StoreLogo.png' = 50; 'Square44x44Logo.png' = 44; 'Square150x150Logo.png' = 150 }

try {
    $payload = & "$PSScriptRoot/Build-Windows.ps1" -PayloadOnly -CargoAbout $CargoAbout -OutputDirectory $stage
    [xml] $manifest = Get-Content -LiteralPath "$PSScriptRoot/msix/AppxManifest.xml" -Raw
    $manifest.Package.Identity.SetAttribute('Name', $PackageName)
    $manifest.Package.Identity.SetAttribute('Publisher', $Publisher)
    $manifest.Package.Identity.SetAttribute('Version', "$($payload.Version).0")
    $manifest.Package.Properties.PublisherDisplayName = $PublisherDisplayName
    $manifest.Save($manifestPath)

    $files = [ordered]@{
        'AppxManifest.xml' = $manifestPath
        'redbim.exe' = $payload.Binary
        'libunwind.dll' = $payload.Runtime
        'Licencia-RedBeam.txt' = (Join-Path $repository 'LICENSE')
        'Licencias-de-terceros.html' = $payload.Notices
        'Licencia-LLVM.txt' = $payload.LlvmLicense
    }
    Add-Type -AssemblyName System.Drawing
    $logo = [Drawing.Image]::FromFile((Join-Path $repository 'assets/redbim_icon.png'))
    try {
        foreach ($name in $logoSizes.Keys) {
            $size = $logoSizes[$name]
            $path = Join-Path $stage $name
            $bitmap = [Drawing.Bitmap]::new($size, $size)
            try {
                $graphics = [Drawing.Graphics]::FromImage($bitmap)
                try {
                    $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                    $graphics.DrawImage($logo, 0, 0, $size, $size)
                } finally { $graphics.Dispose() }
                $bitmap.Save($path, [Drawing.Imaging.ImageFormat]::Png)
            } finally { $bitmap.Dispose() }
            $files["Assets\$name"] = $path
        }
    } finally { $logo.Dispose() }

    $mapping = @('[Files]') + @($files.Keys | ForEach-Object { '"{0}" "{1}"' -f $files[$_], $_ })
    Set-Content -LiteralPath $mappingPath -Value $mapping -Encoding utf8BOM
    $arguments = @('pack', '/f', $mappingPath, '/p', $candidate)
    $rtk = Get-Command rtk -ErrorAction SilentlyContinue
    if ($rtk) { & $rtk.Source $MakeAppx @arguments } else { & $MakeAppx @arguments }
    if ($LASTEXITCODE -ne 0) { throw "MakeAppx terminó con código $LASTEXITCODE." }
    $package = Join-Path $OutputDirectory 'RedBeam-Store-x64.msix'
    [IO.File]::Move($candidate, $package, $true)
    Write-Host "RedBeam $($payload.Version) para Microsoft Store: $package"
} finally {
    $temporaryFiles = @($manifestPath, $mappingPath, $candidate, (Join-Path $stage 'Licencias-de-terceros.html'))
    $temporaryFiles += $logoSizes.Keys | ForEach-Object { Join-Path $stage $_ }
    foreach ($file in $temporaryFiles) {
        if (Test-Path -LiteralPath $file) { Remove-Item -LiteralPath $file }
    }
    [IO.Directory]::Delete($stage)
}
