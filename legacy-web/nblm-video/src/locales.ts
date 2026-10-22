export interface LocaleContent {
  title: string;
  subtitle: string;
  version: string;
  keyFeatures: string;
  features: {
    title1: string;
    desc1: string;
    title2: string;
    desc2: string;
    title3: string;
    desc3: string;
  };
  seeTheDifference: string;
  liteModel: string;
  liteModelDesc: string;
  standardModel: string;
  standardModelDesc: string;
  hybridExtraction: string;
  originalPdf: string;
  pptxOutput: string;
  pdfJsDesc: string;
  workflowTitle: string;
  steps: {
    step1: string;
    step2: string;
    step3: string;
    step4: string;
  };
  workflowNote: string;
  ctaTitle: string;
  ctaSubtitle: string;
}

export const locales: Record<string, LocaleContent> = {
  en: {
    title: "NBLM2PPTX",
    subtitle: "NotebookLM PDF to PPTX Converter",
    version: "v2.3 - Dual Mode OCR System",
    keyFeatures: "Key Features",
    features: {
      title1: "⚡ Dual Mode OCR",
      desc1: "Lite Model: 40-50% faster with 50% API savings\nStandard Model: Full font style detection",
      title2: "🚀 Parallel Processing",
      desc2: "Text removal and OCR run simultaneously\nReduced from 3-4s to 2-3s per page",
      title3: "🌍 Multi-Language",
      desc3: "Support for Traditional Chinese, English, Japanese, Spanish, French\nProfessional light theme design across all versions",
    },
    seeTheDifference: "See the Difference",
    liteModel: "Lite Model ⚡",
    liteModelDesc: "40-50% faster • Saves 50% API quota\nUniform styling (no font variation)",
    standardModel: "Standard Model ✨",
    standardModelDesc: "Full font style detection\nPreserves title/body hierarchy",
    hybridExtraction: "v1.1 - Hybrid Text Extraction",
    originalPdf: "Original PDF",
    pptxOutput: "Editable PPTX Output",
    pdfJsDesc: "PDF.js native extraction for precise positioning",
    workflowTitle: "Simple 4-Step Workflow",
    steps: {
      step1: "Upload PDF or Images",
      step2: "Select Pages to Process",
      step3: "AI Removes Text & Reconstructs Background",
      step4: "Export Editable PPTX",
    },
    workflowNote: "Each slide has clean background + editable text layers",
    ctaTitle: "Try NBLM2PPTX Today!",
    ctaSubtitle: "Free API Quota: 15 requests/min, 1,500 requests/day",
  },
  "zh-TW": {
    title: "NBLM2PPTX",
    subtitle: "NotebookLM PDF 轉 PPTX 轉換器",
    version: "v2.3 - 雙模式 OCR 系統",
    keyFeatures: "主要功能",
    features: {
      title1: "⚡ 雙模式 OCR",
      desc1: "Lite 模型：快 40-50%，節省 50% API 配額\nStandard 模型：完整字體樣式偵測",
      title2: "🚀 平行處理",
      desc2: "文字移除與 OCR 同時執行\n每頁處理時間從 3-4 秒降至 2-3 秒",
      title3: "🌍 多語言支援",
      desc3: "支援繁體中文、英文、日文、西班牙文、法文\n所有版本皆採用專業亮色主題設計",
    },
    seeTheDifference: "看看差別",
    liteModel: "Lite 模型 ⚡",
    liteModelDesc: "快 40-50% • 節省 50% API 配額\n統一樣式（無字體變化）",
    standardModel: "Standard 模型 ✨",
    standardModelDesc: "完整字體樣式偵測\n保留標題/正文層級",
    hybridExtraction: "v1.1 - 混合式文字擷取",
    originalPdf: "原始 PDF",
    pptxOutput: "可編輯 PPTX 輸出",
    pdfJsDesc: "PDF.js 原生擷取，精準定位",
    workflowTitle: "簡單 4 步工作流程",
    steps: {
      step1: "上傳 PDF 或圖片",
      step2: "選擇要處理的頁面",
      step3: "AI 移除文字並重建背景",
      step4: "匯出可編輯 PPTX",
    },
    workflowNote: "每張投影片都有乾淨背景 + 可編輯文字圖層",
    ctaTitle: "立即試用 NBLM2PPTX！",
    ctaSubtitle: "免費 API 配額：每分鐘 15 次，每天 1,500 次",
  },
  ja: {
    title: "NBLM2PPTX",
    subtitle: "NotebookLM PDF から PPTX へのコンバーター",
    version: "v2.3 - デュアルモード OCR システム",
    keyFeatures: "主な機能",
    features: {
      title1: "⚡ デュアルモード OCR",
      desc1: "Lite モデル：40-50% 高速、API を 50% 節約\nStandard モデル：完全なフォントスタイル検出",
      title2: "🚀 並列処理",
      desc2: "テキスト削除と OCR を同時実行\n1 ページあたり 3-4 秒から 2-3 秒に短縮",
      title3: "🌍 多言語対応",
      desc3: "繁体字中国語、英語、日本語、スペイン語、フランス語をサポート\nすべてのバージョンでプロフェッショナルなライトテーマ",
    },
    seeTheDifference: "違いを見てみましょう",
    liteModel: "Lite モデル ⚡",
    liteModelDesc: "40-50% 高速 • API を 50% 節約\n統一スタイル（フォントバリエーションなし）",
    standardModel: "Standard モデル ✨",
    standardModelDesc: "完全なフォントスタイル検出\nタイトル/本文の階層を保持",
    hybridExtraction: "v1.1 - ハイブリッドテキスト抽出",
    originalPdf: "元の PDF",
    pptxOutput: "編集可能な PPTX 出力",
    pdfJsDesc: "PDF.js ネイティブ抽出で正確な配置",
    workflowTitle: "シンプルな 4 ステップワークフロー",
    steps: {
      step1: "PDF または画像をアップロード",
      step2: "処理するページを選択",
      step3: "AI がテキストを削除して背景を再構築",
      step4: "編集可能な PPTX をエクスポート",
    },
    workflowNote: "各スライドにはクリーンな背景と編集可能なテキストレイヤーがあります",
    ctaTitle: "NBLM2PPTX を今すぐお試しください！",
    ctaSubtitle: "無料 API 割り当て：毎分 15 回、毎日 1,500 回",
  },
  es: {
    title: "NBLM2PPTX",
    subtitle: "Conversor de PDF de NotebookLM a PPTX",
    version: "v2.3 - Sistema OCR de Modo Dual",
    keyFeatures: "Características Clave",
    features: {
      title1: "⚡ OCR de Modo Dual",
      desc1: "Modelo Lite: 40-50% más rápido con 50% de ahorro de API\nModelo Standard: Detección completa de estilos de fuente",
      title2: "🚀 Procesamiento Paralelo",
      desc2: "Eliminación de texto y OCR se ejecutan simultáneamente\nReducido de 3-4s a 2-3s por página",
      title3: "🌍 Multi-Idioma",
      desc3: "Soporte para chino tradicional, inglés, japonés, español, francés\nDiseño de tema claro profesional en todas las versiones",
    },
    seeTheDifference: "Vea la Diferencia",
    liteModel: "Modelo Lite ⚡",
    liteModelDesc: "40-50% más rápido • Ahorra 50% de cuota de API\nEstilo uniforme (sin variación de fuente)",
    standardModel: "Modelo Standard ✨",
    standardModelDesc: "Detección completa de estilo de fuente\nPreserva jerarquía de título/cuerpo",
    hybridExtraction: "v1.1 - Extracción Híbrida de Texto",
    originalPdf: "PDF Original",
    pptxOutput: "Salida PPTX Editable",
    pdfJsDesc: "Extracción nativa de PDF.js para posicionamiento preciso",
    workflowTitle: "Flujo de Trabajo Simple de 4 Pasos",
    steps: {
      step1: "Subir PDF o Imágenes",
      step2: "Seleccionar Páginas a Procesar",
      step3: "AI Elimina Texto y Reconstruye Fondo",
      step4: "Exportar PPTX Editable",
    },
    workflowNote: "Cada diapositiva tiene fondo limpio + capas de texto editables",
    ctaTitle: "¡Prueba NBLM2PPTX Hoy!",
    ctaSubtitle: "Cuota gratuita de API: 15 solicitudes/min, 1,500 solicitudes/día",
  },
  fr: {
    title: "NBLM2PPTX",
    subtitle: "Convertisseur PDF NotebookLM vers PPTX",
    version: "v2.3 - Système OCR en Mode Double",
    keyFeatures: "Fonctionnalités Clés",
    features: {
      title1: "⚡ OCR en Mode Double",
      desc1: "Modèle Lite : 40-50% plus rapide avec 50% d'économie d'API\nModèle Standard : Détection complète des styles de police",
      title2: "🚀 Traitement Parallèle",
      desc2: "Suppression de texte et OCR s'exécutent simultanément\nRéduit de 3-4s à 2-3s par page",
      title3: "🌍 Multi-Langue",
      desc3: "Support pour le chinois traditionnel, l'anglais, le japonais, l'espagnol, le français\nConception de thème clair professionnel dans toutes les versions",
    },
    seeTheDifference: "Voyez la Différence",
    liteModel: "Modèle Lite ⚡",
    liteModelDesc: "40-50% plus rapide • Économise 50% de quota d'API\nStyle uniforme (pas de variation de police)",
    standardModel: "Modèle Standard ✨",
    standardModelDesc: "Détection complète des styles de police\nPréserve la hiérarchie titre/corps",
    hybridExtraction: "v1.1 - Extraction Hybride de Texte",
    originalPdf: "PDF Original",
    pptxOutput: "Sortie PPTX Editable",
    pdfJsDesc: "Extraction native PDF.js pour un positionnement précis",
    workflowTitle: "Flux de Travail Simple en 4 Étapes",
    steps: {
      step1: "Télécharger PDF ou Images",
      step2: "Sélectionner les Pages à Traiter",
      step3: "L'IA Supprime le Texte et Reconstruit le Fond",
      step4: "Exporter PPTX Editable",
    },
    workflowNote: "Chaque diapositive a un fond propre + couches de texte éditables",
    ctaTitle: "Essayez NBLM2PPTX Aujourd'hui!",
    ctaSubtitle: "Quota gratuit d'API : 15 requêtes/min, 1 500 requêtes/jour",
  },
  "zh-CN": {
    title: "NBLM2PPTX",
    subtitle: "NotebookLM PDF 转 PPTX 转换器",
    version: "v2.3 - 双模式 OCR 系统",
    keyFeatures: "主要功能",
    features: {
      title1: "⚡ 双模式 OCR",
      desc1: "Lite 模型：快 40-50%，节省 50% API 配额\nStandard 模型：完整字体样式检测",
      title2: "🚀 并行处理",
      desc2: "文字移除与 OCR 同时执行\n每页处理时间从 3-4 秒降至 2-3 秒",
      title3: "🌍 多语言支持",
      desc3: "支持繁体中文、英文、日文、西班牙文、法文\n所有版本皆采用专业亮色主题设计",
    },
    seeTheDifference: "看看差别",
    liteModel: "Lite 模型 ⚡",
    liteModelDesc: "快 40-50% • 节省 50% API 配额\n统一样式（无字体变化）",
    standardModel: "Standard 模型 ✨",
    standardModelDesc: "完整字体样式检测\n保留标题/正文层级",
    hybridExtraction: "v1.1 - 混合式文字提取",
    originalPdf: "原始 PDF",
    pptxOutput: "可编辑 PPTX 输出",
    pdfJsDesc: "PDF.js 原生提取，精准定位",
    workflowTitle: "简单 4 步工作流程",
    steps: {
      step1: "上传 PDF 或图片",
      step2: "选择要处理的页面",
      step3: "AI 移除文字并重建背景",
      step4: "导出可编辑 PPTX",
    },
    workflowNote: "每张投影片都有干净背景 + 可编辑文字图层",
    ctaTitle: "立即试用 NBLM2PPTX！",
    ctaSubtitle: "免费 API 配额：每分钟 15 次，每天 1,500 次",
  },
};

export type LocaleKey = keyof typeof locales;
