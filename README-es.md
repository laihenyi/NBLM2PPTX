# NBLM2PPTX - Convertidor de PDF de NotebookLM a PPTX

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub stars](https://img.shields.io/github/stars/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/network/members)
[![GitHub issues](https://img.shields.io/github/issues/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/issues)

Convierte PDFs exportados de NotebookLM en presentaciones PPTX con **imágenes de fondo y capas de texto editables separadas**.

> ✨ **Actualización (2026-10-21)**: NBLM2PPTX for macOS v0.1.1: convierte PDF en PPTX editable en tu Mac, sin clave de API y notarizada por Apple. Consulta «App nativa para macOS» más abajo.

[English](README.md) | [繁體中文](README-zh-TW.md) | [简体中文](README-zh-CN.md) | [日本語](README-ja.md) | [Français](README-fr.md)

## App nativa para macOS (sin conexión, Rust)

**NBLM2PPTX for macOS** es una app nativa que se ejecuta por completo en tu Mac: sin clave de API y sin subir ningún archivo. Código fuente: [`macos-app/`](macos-app/).

- **Descarga**: [página de la versión](https://github.com/laihenyi/NBLM2PPTX/releases/tag/n2p-rs-v0.1.1) (`NotebookLM2PPT-notarized.zip`)
- **OCR**: Vision integrado en macOS (en el dispositivo)
- **Eliminación de texto**: borra solo los trazos del texto; los fondos con textura se reconstruyen con LaMa en la GPU (CoreML)
- **Extracción de objetos (experimental)**: iconos e ilustraciones pasan a ser imágenes movibles (MobileSAM)
- **App**: interfaz de arrastrar y soltar, firmada con Developer ID y notarizada por Apple; los modelos (~253 MB) se descargan en el primer uso
- **Requisitos**: Apple Silicon; macOS 12 o posterior para la inferencia en GPU

Compilación, uso por línea de comandos y notas de diseño: [`macos-app/README.md`](macos-app/README.md). Licencias de terceros: [`macos-app/THIRD-PARTY.md`](macos-app/THIRD-PARTY.md).

### Antes y después (versión estándar)

| Original (PDF de NotebookLM) | Resultado (PPTX editable, app para macOS) |
|:---:|:---:|
| <img src="assets/demo-after.png" width="400"> | <img src="assets/demo-native-v0.1.1-alpha6.jpg" width="400"> |
| <img src="assets/demo-v1.1-original.jpg" width="400"> | <img src="assets/demo-native-v0.1.1-kitchen.jpg" width="400"> |

> El texto se convierte en cuadros de texto editables; las ilustraciones quedan en el fondo y se elimina la marca de agua de NotebookLM (resultado renderizado con LibreOffice).
> Limitaciones actuales visibles aquí: no se conservan los colores de resaltado, a veces se pierde la negrita y el OCR puede leer mal algunos caracteres.

### v0.1.1 (2026-10-21)

- **Versión con objetos unas 2 veces más rápida y pico de memoria de ~6 GB a ~1,9 GB**: SAM ahora se ejecuta solo en la CPU (presentación de 11 páginas: 47–86 s, antes 103–183 s)
- El texto de varias líneas se alinea a la izquierda cuando así está en el original (antes siempre se centraba)
- Dilatación de máscaras más rápida en la extracción de objetos
- Interfaz: la sección de modelos indica si la versión GPU de LaMa está lista
- Herramientas de desarrollo: la comprobación de compatibilidad con PowerPoint solo lee y cierra la presentación que abrió ella misma

### v0.1.0 (2026-10-21)

- Primera versión: app de arrastrar y soltar, firmada y notarizada
- Velocidad (Apple Silicon, presentación de 11 páginas): versión estándar 29–45 s (~1,1–1,4 GB), versión con objetos 103–183 s (~6 GB)

## Versión web asistida por Gemini (ya no se actualiza)

La versión web anterior asistida por Gemini (`index.html` y sus versiones por idioma, y el proyecto del video de demostración) está archivada en [`legacy-web/`](legacy-web/). **Ya no se actualiza**; los desarrolladores que la necesiten pueden tomarla de allí. Documentación: [`legacy-web/README-es.md`](legacy-web/README-es.md).

## Licencia

MIT License
