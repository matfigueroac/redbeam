# RedBeam en Microsoft Store

Microsoft firma el paquete MSIX cuando lo aprueba y distribuye las actualizaciones desde la tienda. El instalador de GitHub usa su propio mecanismo de actualización. [Documentación de Microsoft](https://learn.microsoft.com/windows/apps/publish/get-started)

## Preparar la publicación

1. Registra o activa la cuenta de desarrollador de Microsoft Store. Microsoft verifica la identidad del titular. [Registro](https://storedeveloper.microsoft.com/)
2. Reserva el nombre de la aplicación en Partner Center. En **Identidad del producto**, copia **Package/Identity/Name**, **Package/Identity/Publisher** y **Package/Properties/PublisherDisplayName**. [Identidad del paquete](https://learn.microsoft.com/windows/apps/publish/store-readiness-checklist)
3. En PowerShell 7, con las herramientas de la [compilación para Windows](publicar.md) y MakeAppx del SDK de Windows, genera el paquete con esos valores.

```powershell
$identity = Read-Host 'Package/Identity/Name'
$publisher = Read-Host 'Package/Identity/Publisher'
$displayName = Read-Host 'Package/Properties/PublisherDisplayName'
./installer/Build-Store.ps1 -CargoAbout $tools.CargoAbout -MakeAppx makeappx.exe `
  -PackageName $identity -Publisher $publisher -PublisherDisplayName $displayName
```

Usa la ruta completa en `-MakeAppx` si la herramienta queda fuera de `PATH`. El archivo generado es `dist/store/RedBeam-Store-x64.msix`. Mantén la misma identidad en las siguientes versiones y aumenta la versión de `Cargo.toml`.

4. Sube el `.msix` en Partner Center. Agrega la descripción y las capturas, completa el cuestionario de clasificación y envía la versión a certificación. [Proceso de publicación](https://learn.microsoft.com/windows/apps/publish/publish-your-app/create-app-submission)

## Ficha en español

RedBeam es una aplicación de escritorio para revisar, anotar y cubicar planos PDF. Permite calibrar la escala, medir longitudes y áreas, contar elementos y exportar resultados. Incluye un plano de ejemplo para practicar y admite unidades métricas e imperiales.

- Captura de escritorio para la ficha. Usa `target/release/redbim.exe --demo --screenshot plano-demo.png` para generar en PNG la [vista del plano de ejemplo](img/plano-demo.jpg). [Requisitos de imágenes](https://learn.microsoft.com/windows/apps/publish/publish-your-app/msix/screenshots-and-images)
- [Política de privacidad](privacidad.md)
- [Soporte](https://github.com/matfigueroac/redbeam/issues)

Para la certificación, el equipo de Microsoft puede abrir el plano de ejemplo desde el inicio. El paquete solicita `runFullTrust` para ejecutar la aplicación de escritorio Win32, abrir y guardar los PDF que selecciona el usuario y usar los diálogos de Windows.
