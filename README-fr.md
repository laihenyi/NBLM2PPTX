# NBLM2PPTX - Convertisseur PDF NotebookLM vers PPTX

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub stars](https://img.shields.io/github/stars/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/network/members)
[![GitHub issues](https://img.shields.io/github/issues/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/issues)

Convertissez les PDF exportés de NotebookLM en présentations PPTX avec **images d'arrière-plan et couches de texte éditables séparées**.

> ✨ **Mise à jour (2026-10-21)** : NBLM2PPTX for macOS v0.1.1 — conversion de PDF en PPTX modifiable sur votre Mac, sans clé d’API, notarisée par Apple. Voir « Application native macOS » ci-dessous.

[English](README.md) | [繁體中文](README-zh-TW.md) | [简体中文](README-zh-CN.md) | [日本語](README-ja.md) | [Español](README-es.md)

## Application native macOS (hors ligne, Rust)

**NBLM2PPTX for macOS** est une application native qui s’exécute entièrement sur votre Mac : aucune clé d’API, aucun fichier envoyé. Code source : [`macos-app/`](macos-app/).

- **Téléchargement** : [page de la version](https://github.com/laihenyi/NBLM2PPTX/releases/tag/n2p-rs-v0.1.1) (`NotebookLM2PPT-notarized.zip`)
- **OCR** : Vision intégré à macOS (sur l’appareil)
- **Suppression du texte** : seuls les traits du texte sont effacés ; les arrière-plans texturés sont reconstruits par LaMa sur le GPU (CoreML)
- **Extraction d’objets (expérimental)** : les icônes et illustrations deviennent des images déplaçables (MobileSAM)
- **Application** : interface glisser-déposer, signée avec un Developer ID et notarisée par Apple ; les modèles (~253 Mo) sont téléchargés au premier lancement
- **Configuration requise** : Apple Silicon ; macOS 12 ou ultérieur pour l’inférence sur GPU

Compilation, utilisation en ligne de commande et notes de conception : [`macos-app/README.md`](macos-app/README.md). Licences tierces : [`macos-app/THIRD-PARTY.md`](macos-app/THIRD-PARTY.md).

### Avant / après (version standard)

| Original (PDF NotebookLM) | Résultat (PPTX modifiable, application macOS) |
|:---:|:---:|
| <img src="assets/demo-after.png" width="400"> | <img src="assets/demo-native-v0.1.1-alpha6.jpg" width="400"> |
| <img src="assets/demo-v1.1-original.jpg" width="400"> | <img src="assets/demo-native-v0.1.1-kitchen.jpg" width="400"> |

> Le texte devient des zones de texte modifiables ; les illustrations restent en arrière-plan et le filigrane NotebookLM est supprimé (rendu avec LibreOffice).
> Limites actuelles visibles ici : les couleurs de surlignage ne sont pas conservées, le gras est parfois perdu et l’OCR peut mal lire quelques caractères.

### v0.1.1 (2026-10-21)

- **Version avec objets environ 2 fois plus rapide, pic mémoire de ~6 Go à ~1,9 Go** : SAM s’exécute désormais uniquement sur le CPU (présentation de 11 pages : 47–86 s, contre 103–183 s)
- Le texte sur plusieurs lignes est aligné à gauche lorsque l’original l’est (il était toujours centré)
- Dilatation des masques plus rapide pour l’extraction d’objets
- Interface : la section des modèles indique si la version GPU de LaMa est prête
- Outils de développement : la vérification de compatibilité PowerPoint ne lit et ne ferme que la présentation qu’elle a elle-même ouverte

### v0.1.0 (2026-10-21)

- Première version : application glisser-déposer, signée et notarisée
- Vitesse (Apple Silicon, présentation de 11 pages) : version standard 29–45 s (~1,1–1,4 Go), version avec objets 103–183 s (~6 Go)

## Version web assistée par Gemini (plus mise à jour)

L’ancienne version web assistée par Gemini (`index.html` et ses versions linguistiques, ainsi que le projet de vidéo de démonstration) est archivée dans [`legacy-web/`](legacy-web/). Elle **n’est plus mise à jour** ; les développeurs qui en ont besoin peuvent la récupérer là. Documentation : [`legacy-web/README-fr.md`](legacy-web/README-fr.md).

## Licence

MIT License
