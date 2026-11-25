# Compilar y publicar RedBIM

## Compilación local en Windows

Usa PowerShell 7, Rustup y LLVM-MinGW UCRT x64. El flujo de GitHub fija Rust 1.99.1 y LLVM-MinGW 20260908. El script descarga y comprueba los hashes del compilador de instaladores y del generador de avisos de licencia.

```powershell
rustup toolchain install 1.99.1-x86_64-pc-windows-gnullvm --profile minimal
$env:RUSTUP_TOOLCHAIN = '1.99.1-x86_64-pc-windows-gnullvm'
$tools = ./installer/Get-Tools.ps1 -Destination ./work/distribution/tools -IncludeLlvm
$env:PATH = "$($tools.LlvmBin);$env:PATH"

cargo test --locked --release --lib --tests
./installer/Build-Windows.ps1 -Iscc $tools.Iscc -CargoAbout $tools.CargoAbout
```

Si LLVM-MinGW UCRT x64 ya está en `PATH`, puedes omitir `-IncludeLlvm` y el cambio de `PATH`. El instalador incluye el runtime de LLVM junto al ejecutable. El paquete, `SHA256SUMS.txt` y los avisos de dependencias quedan en `dist/`.

Para verificar el paquete con una cuenta de prueba de Windows donde RedBIM aún no esté registrado:

```powershell
./installer/Test-Installer.ps1 -Installer ./dist/RedBIM-Setup-x64.exe
Get-FileHash -Algorithm SHA256 ./dist/RedBIM-Setup-x64.exe
```

La prueba instala, ejecuta el programa con un `PATH` de sistema, reinstala en la misma ubicación y desinstala. También comprueba que un PDF de prueba sobrevive al proceso. Pasa `-PreviousInstaller` con el instalador de la última versión para probar el salto entre versiones.

## Preparar una versión

1. Actualiza `version` en `Cargo.toml`, sincroniza `Cargo.lock` con `cargo check` y agrega los cambios a `CHANGELOG.md`.
2. Revisa `git status`, el contenido que se publicará y los resultados de la prueba local. Los informes locales con rutas a planos de prueba, `work/` y los binarios de `dist/` quedan fuera de Git. Ejecuta el flujo **Windows** en GitHub Actions para comprobar el paquete en el entorno de CI.
3. Con el commit en `main` y los cambios revisados, crea y sube la etiqueta `vX.Y.Z` que corresponde a la versión de `Cargo.toml`.

```powershell
git tag -a v0.2.0 -m "RedBIM 0.2.0"
git push origin v0.2.0
```

Para `matfigueroac/redbim`, la etiqueta activa las pruebas, el empaquetado, la validación del instalador y la publicación en GitHub Releases. Las copias instaladas consultan `releases/latest` al abrirse y comprueban el SHA-256 del instalador antes de ejecutarlo. La descarga conserva el mismo nombre entre versiones. GitHub reserva `releases/latest` para publicaciones completas. Si cambia el repositorio de distribución, actualiza las URLs del README, `src/updates.rs`, el instalador y la condición de publicación del flujo. [Documentación de GitHub Releases](https://docs.github.com/en/rest/releases/releases#get-the-latest-release)

## Ejecutar desde el código fuente

```powershell
cargo run --locked --release -- --demo
cargo run --locked --release -- "ruta\al\plano.pdf"
```
