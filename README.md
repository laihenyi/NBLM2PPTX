# NBLM2PPTX - NotebookLM PDF to PPTX Converter

Convert NotebookLM exported PDFs into PPTX presentations with **separated background images and editable text layers**.

[繁體中文](README-zh-TW.md) | [简体中文](README-zh-CN.md) | [日本語](README-ja.md) | [Español](README-es.md) | [Français](README-fr.md)

## Demo

| Before (NotebookLM PDF) | After (Editable PPTX) |
|:-----------------------:|:---------------------:|
| ![Before](assets/demo-before.png) | ![After](assets/demo-after.png) |

> Left: Original PDF from NotebookLM (text embedded in image)
> Right: Converted PPTX with clean background + editable text layers

## Features

- **AI Text Removal**: Uses Gemini 2.5 Flash to automatically remove text from images and reconstruct backgrounds
- **OCR Text Positioning**: Accurately recognizes original text positions, sizes, and colors
- **Separated Layers**: Exported PPTX contains background images and text as independent layers for easy editing
- **Batch Processing**: Supports processing multiple PDF pages or images at once
- **Page Selection**: Freely select which pages to process, saving time and API quota

## Usage

### Using in Google Gemini Canvas

1. Open [Google Gemini](https://gemini.google.com/)
2. Enter a prompt like:
   ```
   Help me create a web tool to convert PDF to PPTX
   ```
3. When Gemini enters **Canvas mode** (code editor appears on the right side)
4. Paste the complete code from the project's `index.html` (or your preferred language version) into Canvas
5. Click the "**Preview**" button in the top-right corner of Canvas to run

### API Key Configuration

> **Important**: When running in Gemini Canvas environment, **no personal API Key is required**. The system will use the default API environment automatically.

If you want to run the tool outside of Canvas (e.g., on your own server), find the following line in the code and enter your Gemini API Key:

```javascript
const apiKey = "YOUR_GEMINI_API_KEY";
```

> Get an API Key: Visit [Google AI Studio](https://aistudio.google.com/app/apikey)

## Workflow

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│  Upload PDF │ -> │   Select    │ -> │ AI Process  │ -> │ Export PPTX │
│  or Images  │    │   Pages     │    │ Remove Text │    │ BG + Text   │
└─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘
```

### Step 1: Upload Files
- Drag and drop or click to upload NotebookLM exported PDFs
- Also supports JPG, PNG, WebP and other image formats
- Multiple files can be uploaded at once

### Step 2: Select Pages
- System automatically generates thumbnails for all pages
- Check the pages you want to process (all selected by default)
- Click "Start Processing" to proceed

### Step 3: AI Processing
- Gemini removes text from each page and reconstructs the background
- Progress is displayed in real-time
- Each page takes approximately 3-5 seconds (including API latency)

### Step 4: Export PPTX
- Select presentation ratio (16:9 / 9:16 / 4:3)
- Click "Export PPTX" to download
- OCR is performed again during export to position text accurately

## Output Structure

Each slide in the exported PPTX contains:

| Layer | Content |
|-------|---------|
| Bottom | Clean background image with text removed |
| Top | Editable text boxes (positioned to match original text) |

This layered structure allows you to:
- Easily modify text content
- Change fonts, colors, and sizes
- Adjust text positions
- Preserve the original design style

## Technical Specifications

| Item | Description |
|------|-------------|
| AI Model | Gemini 2.5 Flash (Image Edit + Text Gen) |
| PDF Parsing | PDF.js 3.11.174 |
| PPTX Generation | PptxGenJS 3.12.0 |
| Render Resolution | Thumbnail 0.5x / Processing 2.0x |
| Supported Formats | PDF, JPG, PNG, WebP, BMP |

## Notes

1. **API Quota**: Each processing call uses Gemini API; monitor your usage if running outside Canvas
2. **Rate Limiting**: System automatically waits and retries on 429 errors
3. **Processing Time**: For large numbers of pages, consider processing in batches
4. **Network**: Requires stable internet connection
5. **Browser**: Chrome or Edge (latest version) recommended

## FAQ

### Q: Why use Gemini Canvas?
A: Canvas mode provides a secure sandbox environment to run frontend code without setting up a server. Plus, it uses the default API environment, so no personal API Key is needed.

### Q: What if processing fails?
A: Common causes:
- Invalid or expired API Key (when running outside Canvas)
- Unstable network connection
- Image too large or unsupported format
- API rate limit exceeded (wait and retry)

### Q: Can it be used offline?
A: No, this tool requires Gemini API calls for AI processing.

## Language Versions

| Language | File |
|----------|------|
| 繁體中文 | `index.html` |
| English | `index-en.html` |
| Español | `index-es.html` |
| 日本語 | `index-ja.html` |
| Français | `index-fr.html` |
| 简体中文 | `index-zh-CN.html` |

## License

MIT License
