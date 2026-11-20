# RedBIM — Visor, Marcado y Cubicación de Planos PDF para Windows

**RedBIM** es una aplicación de escritorio nativa, ligera y acelerada por GPU escrita en **Rust**, diseñada para revisión de planos de arquitectura, ingeniería y construcción (AEC) con **enfoque por defecto en Sistema Métrico Chileno / SI (`m`, `cm`, `mm`, `m²`, `m³`, láminas ISO `A0`–`A4`)** y soporte completo para **medidas imperiales de EE.UU. (`ft-in` fraccionario, `in`, `ft²`, `yd³`)**.

Su arquitectura de anotaciones, mediciones y flujos de trabajo fue modelada mediante análisis estático (REA) sobre **Bluebeam Revu 21** (`Revu32.exe` v21.11.0.23287 y sus ensamblados `.Model.Core`, `.Model.PDF.Core`, `.ViewModel.Core` y `Bluebeam.Types.Core`), conservando las herramientas clave de revisión y cubicación, mejorando la limpieza visual de la interfaz y **excluyendo** explícitamente Bluebeam Studio, cuentas en la nube, suscripciones, publicidad, integraciones Office/SharePoint y tutoriales invasivos.

---

## Diferenciador Principal: Enfoque Chile / Sistema Métrico por Defecto

| Aspecto | Enfoque tradicional EE.UU. | RedBIM (Enfoque Chile / Métrico) |
| :--- | :--- | :--- |
| **Unidad de longitud por defecto** | Pulgadas / Pies (`ft-in`) | **Metros (`m`)**, con selector rápido a `cm`, `mm`, `km` y EE.UU. (`ft`, `in`, `ft-in`, `yd`) |
| **Área y volumen por defecto** | `sq ft` / `cu yd` | **`m²`** (`cm²`, `mm²`, `ha`) y **`m³`** (`cm³`, `L`) |
| **Tamaño de lámina en barra de estado** | Pulgadas (`46.81 × 33.11 in`) | **Milímetros + formato ISO** (`841 × 594 mm (A1)`), alternable a `cm` o `in` con un clic |
| **Escalas preestablecidas** | Imperiales primero | **Métricas primero (`1:1` a `1:5000`)**, calibración 2 puntos y luego Imperiales EE.UU. |
| **Pendiente** | Pitch `x:12` | **Porcentaje (`%`)**, Grados (`°`) o Pitch `x:12` |
| **Exportación CSV (Excel Chile)** | Delimitador `,` | **UTF-8 con BOM y delimitador `;`** por defecto (o `,` configurable) |

---

## Funcionalidades Implementadas

### 1. Comentarios, Flechas, Nubes y Marcas
- **Notas adhesivas** (`N`) con 15 estilos de ícono (`Comentario`, `Nota`, `Aprobado`, `Cruz`, `Consulta ?`, `Clave`, `Destacado`, etc.).
- **Cuadros de texto** (`T`) con formas configurables (Rectángulo, Círculo/Óvalo, Triángulo, Hexágono) y **Máquina de escribir** (`W`).
- **Llamadas con flecha (`Callout`, `Q`)**: punta de flecha, codo articulado, caja de texto con ajuste automático y edición directa en el plano con doble clic.
- **Nube (`C`)** (arrastrar rectángulo o clic a clic poligonal con cierre por doble clic / `Enter`) y **Nube+ (`K`, nube con llamada y comentario pareado)** con arcos semicirculares hacia el exterior.
- **Flechas (`A`), Líneas (`L`), Polilíneas (`Shift+N`), Arcos de 3 puntos (`Shift+C`), Rectángulos (`R`), Elipses (`E`), Polígonos (`P`), Lápiz mano alzada, Resaltador (`H`), Sellos (`S`), Imágenes (render real PNG/JPEG y PDF aplanado) e Instantánea (`G`, captura al portapapeles PNG)**.
- **Alineación y distribución múltiple**: alinear a izquierda/centro/derecha/arriba/medio/abajo y distribuir horizontal/verticalmente desde el menú `Editar` o menú contextual.
- **10 estilos de extremos de línea**: Ninguno, Flecha abierta/rellena, Flecha invertida, Diagonal (`/`), Tope, Círculo, Cuadrado, Diamante.
- **Patrones de achurado**: Diagonal 45°, Diagonal 135°, Cuadrícula 45°, Horizontal y Vertical, con recorte automático de descuentos (*cutouts*).
- **Hilo de respuestas, Estado** (`Pendiente`, `Aceptado`, `Rechazado`, `Completado`, `Anulado`) y **Capas** independientes.

### 2. Escala, Calibración y Mediciones
- **Calibración de escala en 2 puntos** con **lupa de precisión en vivo**, **banner rápido de calibración** sobre planos sin escala (`1:50`, `1:100`, `1:200`) y aplicación por página, rango (`"1, 3-5"`) o documento completo.
- **Escala X / Y independiente** y **Viewports de escala editables** dentro de una misma lámina (por ejemplo, planta `1:50` con detalle `1:20` en la misma hoja).
- **Herramientas de medición y presupuesto**:
  - **Longitud (`Shift+L`)** con líneas de extensión, desfase ajustable y pendiente/desnivel opcional
  - **Polilongitud** y **Perímetro (`Shift+P`)** con **altura de muro y Área de muro (`m²`)** y cotas por segmento opcionales
  - **Área (`Shift+A`)** y **Volumen (`Shift+V`)** con soporte de **Recortes / Descuentos (`Cutout`)** y profundidad en metros
  - **Ángulo (`Shift+G`)**, **Radio (`Shift+U`)**, **Diámetro** y **Conteo (`Shift+O`)** numerado
  - **Precio unitario (`$`) y subtotal de cubicación** en Propiedades, Lista de marcas y CSV
  - **Dibujo a escala exacto** (rectángulo por ancho y alto en `m`/`cm`/`mm`/`ft-in`)
  - **Leyenda de mediciones** viva sobre la lámina

### 3. Productividad y Motor PDF Nativo
- **Motor PDF 100% puro Rust** (`hayro` + `lopdf`, sin dependencias externas ni DLLs):
  - Renderizado por **tiles de alta resolución** al hacer zoom profundo.
  - **Snap al Contenido PDF** (`hayro_interpret::Device` extrayendo vértices, puntos finales y puntos medios de la geometría vectorial del plano), **Snap a Marcas**, **Snap a Grilla** y bloqueo ortogonal 45°/90° con `Shift`.
  - **Comparación de documentos PDF** (superposición visual rojo/azul de cambios + nubes automáticas de diferencias), **Combinar PDFs** y **Extraer página actual**.
  - Guardado con **`/Annots` + `/AP` (Appearance Streams) + `/VP` + `/Measure` estándar ISO 32000-1** más estado lossless **`/RedBIMState`**.
  - Exportación a **PDF Aplanado** (marcas y mediciones dibujadas vectorialmente en el flujo de página) y **CSV / JSON**.
- **Caja de herramientas (`Tool Chest`)**: `Mis herramientas` (atajos `1`–`8`), `Recientes`, `Arquitecto revisor`, `Contratista revisor`, `Ingeniero revisor`, `Mediciones generales (Chile)` y `Símbolos y sellos`, con soporte de **Modo Dibujo / Modo Propiedades** y persistencia local.
- **Lista de marcas** inferior ordenable y filtrable con edición de estado inline, salto directo al hacer clic y fila de totales.
- **Vista dividida** vertical u horizontal con cámaras independientes, **Atenuador (`Dimmer`)**, **Miniaturas**, **Capas**, **Marcadores**, **Búsqueda de texto en PDF y marcas**, y **Temas Oscuro y Claro**.

---

## Compilación y Ejecución

Requisitos: Rust (`cargo` / `rustc` 1.88+ o toolchain `stable-x86_64-pc-windows-gnullvm`).

```powershell
# Ejecutar tests unitarios y de integración
cargo test

# Compilar binario nativo Windows optimizado
cargo build --release

# Abrir RedBIM con el plano de ejemplo chileno (3 láminas A1, escala 1:100 / 1:50 / 1:25)
.\target\release\redbim.exe --demo --measurements

# Abrir un PDF específico
.\target\release\redbim.exe "ruta\al\plano.pdf"

# Iniciar en tema claro
.\target\release\redbim.exe --demo --light
```

También incluye un plano de ejemplo generado en [`samples/plano_demo_chile.pdf`](samples/plano_demo_chile.pdf).

---

## Atajos de Teclado Principales

| Atajo | Acción |
| :--- | :--- |
| `V` / `Esc` | Seleccionar / Cancelar herramienta |
| `N` / `T` / `W` | Nota / Cuadro de texto / Máquina de escribir |
| `Q` / `C` / `K` | Llamada (`Callout`) / Nube / Nube+ |
| `A` / `L` / `R` / `E` / `P` | Flecha / Línea / Rectángulo / Elipse / Polígono |
| `H` / `S` / `G` | Resaltador / Sello / Instantánea PNG al portapapeles |
| `Shift+L` / `Shift+P` / `Shift+A` / `Shift+V` | Longitud / Perímetro / Área / Volumen |
| `Shift+G` / `Shift+U` / `Shift+O` | Ángulo / Radio / Conteo |
| `1` – `8` | Activar herramienta de *Mis herramientas* |
| `Doble clic` sobre texto/nota | Editar comentario directamente en el plano |
| `Rueda` / `Espacio + Arrastrar` | Zoom al cursor / Desplazar lámina |
| `Ctrl+O` / `Ctrl+S` / `Ctrl+Z` / `Ctrl+Y` | Abrir / Guardar / Deshacer / Rehacer |
| `Ctrl+0` / `Ctrl+1` | Ajustar página / Zoom 100% |
