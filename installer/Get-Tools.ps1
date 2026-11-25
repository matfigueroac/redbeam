[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $Destination,
    [switch] $IncludeLlvm
)

$ErrorActionPreference = 'Stop'
$Destination = [IO.Path]::GetFullPath($Destination)
New-Item -ItemType Directory -Path $Destination -Force | Out-Null

function Get-VerifiedFile([string] $Url, [string] $Sha256) {
    $file = Join-Path $Destination ([IO.Path]::GetFileName($Url))
    if (!(Test-Path -LiteralPath $file)) {
        Invoke-WebRequest -Uri $Url -OutFile $file
    }
    if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash -ne $Sha256) {
        throw "SHA-256 incorrecto: $file"
    }
    return $file
}

$innoDir = Join-Path $Destination 'inno-6.7.3'
$iscc = Join-Path $innoDir 'ISCC.exe'
if (!(Test-Path -LiteralPath $iscc)) {
    $innoInstaller = Get-VerifiedFile `
        'https://github.com/jrsoftware/issrc/releases/download/is-6_7_3/innosetup-6.7.3.exe' `
        '9c73c3bae7ed48d44112a0f48e66742c00090bdb5bef71d9d3c056c66e97b732'
    $process = Start-Process -FilePath $innoInstaller -ArgumentList `
        '/PORTABLE=1', '/CURRENTUSER', '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/DIR=`"$innoDir`"" `
        -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0 -or !(Test-Path -LiteralPath $iscc)) {
        throw "Falló la instalación de Inno Setup ($($process.ExitCode))."
    }
}

$aboutDir = Join-Path $Destination 'cargo-about-0.9.3'
$cargoAbout = Join-Path $aboutDir 'cargo-about-0.9.3-x86_64-pc-windows-msvc/cargo-about.exe'
if (!(Test-Path -LiteralPath $cargoAbout)) {
    $aboutArchive = Get-VerifiedFile `
        'https://github.com/EmbarkStudios/cargo-about/releases/download/0.9.3/cargo-about-0.9.3-x86_64-pc-windows-msvc.tar.gz' `
        '9e2942533ef1c592c4a70bdb384cae1550a3f5fc9ae40379b6662e3d2f16fe02'
    New-Item -ItemType Directory -Path $aboutDir -Force | Out-Null
    tar -xf $aboutArchive -C $aboutDir
    if ($LASTEXITCODE -ne 0 -or !(Test-Path -LiteralPath $cargoAbout)) {
        throw 'No se pudo extraer cargo-about.'
    }
}

$llvmBin = $null
if ($IncludeLlvm) {
    $llvmName = 'llvm-mingw-20260908-ucrt-x86_64'
    $llvmBin = Join-Path $Destination "$llvmName/bin"
    if (!(Test-Path -LiteralPath (Join-Path $llvmBin 'clang.exe'))) {
        $llvmArchive = Get-VerifiedFile `
            "https://github.com/mstorsjo/llvm-mingw/releases/download/20260908/$llvmName.zip" `
            '1bcf74d06b724aeecaa6412ca85f5b26fb1da770e7cdcefa9263c9c5c3ad34b6'
        Expand-Archive -LiteralPath $llvmArchive -DestinationPath $Destination -Force
    }
}

[pscustomobject]@{ Iscc = $iscc; CargoAbout = $cargoAbout; LlvmBin = $llvmBin }
