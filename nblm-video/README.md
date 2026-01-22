# NBLM2PPTX Demo Video

A Remotion-based multilingual promotional video for the NBLM2PPTX project - NotebookLM PDF to PPTX Converter.

## 🌍 Supported Languages

| Language | Folder | Composition ID |
|----------|--------|----------------|
| 🇺🇸 English | English | `en-demo` |
| 🇹🇼 繁體中文 | Traditional-Chinese | `zh-TW-demo` |
| 🇨🇳 简体中文 | Simplified-Chinese | `zh-CN-demo` |
| 🇯🇵 日本語 | Japanese | `ja-demo` |
| 🇪🇸 Español | Spanish | `es-demo` |
| 🇫🇷 Français | French | `fr-demo` |

## Project Structure

```
nblm-video/
├── src/
│   ├── Root.tsx          # Composition definitions (multilingual)
│   ├── DemoVideo.tsx     # Main video component (locale-aware)
│   ├── locales.ts        # i18n translations for all languages
│   ├── index.tsx         # Entry point
│   └── index.html        # HTML template
├── public/
│   └── images/           # Demo screenshots
│       ├── demo-v2.3-lite.jpg
│       ├── demo-v2.3-standard.jpg
│       ├── demo-v1.1-original.jpg
│       └── demo-v1.1-output.jpg
├── package.json
├── remotion.config.ts    # Remotion configuration
└── README.md
```

## Video Content

Each language video (40 seconds @ 30fps = 1200 frames) includes:

1. **Title Scene** (0-5s): Project title and version
2. **Features Scene** (5-13s): Key features with animated cards
3. **Demo Comparison Scene** (13-23s): Visual comparisons of v2.3 Lite vs Standard models, and v1.1 Hybrid Text Extraction
4. **Workflow Scene** (23-33s): 4-step workflow visualization
5. **CTA Scene** (33-40s): Call-to-action with GitHub link

### Demo Images Used

- `demo-v2.3-lite.jpg` - Lite model output (faster, uniform styling)
- `demo-v2.3-standard.jpg` - Standard model output (full font style detection)
- `demo-v1.1-original.jpg` - Original PDF from NotebookLM
- `demo-v1.1-output.jpg` - Editable PPTX output with precise text positioning

## Development

Start the Remotion Studio preview:

```bash
cd nblm-video
npm start
```

This opens the studio at `http://localhost:3000` where you can:
- Preview all 6 language versions
- Switch between languages using the sidebar
- Make changes and see instant updates
- Adjust timing and animations

## Rendering

Render a specific language video:

```bash
# English
remotion render en-demo out/en-video.mp4

# Traditional Chinese
remotion render zh-TW-demo out/zh-TW-video.mp4

# Japanese
remotion render ja-demo out/ja-video.mp4
```

Render all languages:

```bash
npm run build:all
```

## Adding New Languages

1. Add translations to `src/locales.ts`:

```typescript
export const locales: Record<string, LocaleContent> = {
  // ... existing languages
  "de": {
    title: "NBLM2PPTX",
    subtitle: "NotebookLM PDF zu PPTX Konverter",
    // ... rest of translations
  },
};
```

2. Add to language list in `src/Root.tsx`:

```typescript
const languages = [
  // ... existing languages
  { key: "de", name: "Deutsch", folder: "German" },
];
```

3. Add corresponding flag to README table.

## Customization

Edit `src/DemoVideo.tsx` to customize:
- Colors and styling
- Animation timing
- Scene durations

Edit `src/locales.ts` to customize:
- Text content for each language
- Translations for all UI elements

## Remotion Best Practices Applied

Based on [Remotion Best Practices](https://github.com/remotion-dev/skills):

- ✅ Using `useCurrentFrame()` for all animations
- ✅ Using `<Img>` component with `staticFile()` for images
- ✅ No CSS transitions or Tailwind animation classes
- ✅ Using `interpolate()` for smooth value transitions
- ✅ Using `spring()` for natural motion
- ✅ Proper TypeScript types for props
- ✅ Frame-based timing (multiplied by fps)
- ✅ i18n with locale-aware components

## Tech Stack

- **Remotion 4.0** - React-based video creation framework
- **React 18** - UI library
- **TypeScript** - Type safety
- **i18n** - Custom locale system with 6 languages
