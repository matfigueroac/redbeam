[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $Installer,
    [string] $PreviousInstaller,
    [string] $ScratchDirectory = $env:TEMP
)

$ErrorActionPreference = 'Stop'
$Installer = (Get-Item -LiteralPath $Installer).FullName
$version = (Get-Item -LiteralPath $Installer).VersionInfo.ProductVersion.TrimEnd(' ', [char]0)
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{C5034EAE-B272-4175-92B5-F6284A1187A1}_is1'
if (Test-Path -LiteralPath $uninstallKey) {
    throw 'Ya existe una instalación de RedBeam registrada en este usuario. Usa una cuenta de prueba para este script.'
}
$scratch = Join-Path ([IO.Path]::GetFullPath($ScratchDirectory)) "redbim-installer-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $scratch -Force | Out-Null
$installed = Join-Path $scratch 'RedBeam de prueba'
$app = Join-Path $installed 'redbim.exe'
$uninstall = Join-Path $installed 'unins000.exe'
$pdf = Join-Path $installed 'Plano de prueba.pdf'
$quiet = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART')

function Start-Checked([string] $Program, [string[]] $Arguments) {
    $process = Start-Process -FilePath $Program -ArgumentList $Arguments -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "$Program terminó con código $($process.ExitCode)." }
}

function Export-Demo([string] $Destination) {
    $originalPath = $env:PATH
    try {
        $env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
        Start-Checked $app @('--export-demo', "`"$Destination`"")
    } finally {
        $env:PATH = $originalPath
    }
    if ((Get-Item -LiteralPath $Destination).Length -lt 1000) {
        throw 'El plano de prueba quedó incompleto.'
    }
}

$installArgs = $quiet + @('/SP-', '/NOICONS', "/DIR=`"$installed`"", "/LOG=`"$scratch/setup.log`"")
$previousPath = $Installer
if ($PreviousInstaller) { $previousPath = (Get-Item -LiteralPath $PreviousInstaller).FullName }
$previousVersion = (Get-Item -LiteralPath $previousPath).VersionInfo.ProductVersion.TrimEnd(' ', [char]0)
if ($PreviousInstaller -and [version]$previousVersion -ge [version]$version) {
    throw 'El instalador anterior debe tener una versión inferior a la del candidato.'
}

try {
    Start-Checked $previousPath $installArgs
    if (!(Test-Path -LiteralPath $app) -or !(Test-Path -LiteralPath $uninstall)) {
        throw 'La instalación no creó el ejecutable o el desinstalador.'
    }

    if ((Get-ItemProperty -LiteralPath $uninstallKey).DisplayVersion -ne $previousVersion) {
        throw 'Windows no registró la versión esperada en la primera instalación.'
    }
    $oldBinaryHash = (Get-FileHash -LiteralPath $app).Hash
    Export-Demo $pdf
    $originalHash = (Get-FileHash -LiteralPath $pdf).Hash

    Start-Checked $Installer ($quiet + @('/SP-', '/NOICONS', "/LOG=`"$scratch/upgrade.log`""))
    if ((Get-Item -LiteralPath $app).VersionInfo.ProductVersion -ne $version) {
        throw 'La versión instalada no coincide con la del instalador.'
    }
    if ((Get-ItemProperty -LiteralPath $uninstallKey).DisplayVersion -ne $version) {
        throw 'Windows no registró la versión esperada después de actualizar.'
    }
    $registered = (Get-ItemProperty -LiteralPath $uninstallKey).InstallLocation
    $registeredDir = [IO.Path]::TrimEndingDirectorySeparator((Get-Item -LiteralPath $registered).FullName)
    $expectedDir = [IO.Path]::TrimEndingDirectorySeparator((Get-Item -LiteralPath $installed).FullName)
    if ($registeredDir -ne $expectedDir) {
        throw "La ubicación registrada ($registeredDir) no coincide con la carpeta de instalación ($expectedDir)."
    }
    if ($PreviousInstaller -and (Get-FileHash -LiteralPath $app).Hash -eq $oldBinaryHash) {
        throw 'La actualización dejó el ejecutable anterior.'
    }
    if ((Get-FileHash -LiteralPath $pdf).Hash -ne $originalHash) { throw 'La actualización modificó el plano de prueba.' }
    foreach ($name in @('libunwind.dll', 'Licencia-RedBeam.txt', 'Licencias-de-terceros.html', 'Licencia-LLVM.txt')) {
        if (!(Test-Path -LiteralPath (Join-Path $installed $name))) { throw "Falta $name en la instalación." }
    }
    $updatedPdf = Join-Path $scratch 'Plano exportado después de actualizar.pdf'
    Export-Demo $updatedPdf
    Remove-Item -LiteralPath $updatedPdf

} finally {
    if (Test-Path -LiteralPath $uninstall) { Start-Checked $uninstall $quiet }
}

if ((Test-Path -LiteralPath $app) -or (Test-Path -LiteralPath $uninstallKey)) {
    throw 'Windows conserva archivos de RedBeam o su entrada de desinstalación.'
}
if ((Get-FileHash -LiteralPath $pdf).Hash -ne $originalHash) { throw 'La desinstalación modificó el plano de prueba.' }
Remove-Item -LiteralPath $pdf
Write-Host "Instalación, actualización, ejecución y desinstalación verificadas. Registros en $scratch"
