"""複現 NotebookLM2PPT 對 Spire.Presentation 的每一項呼叫，把結果輸出成獨立的 pptx，
用來反推 Spire 實際寫出的 OOXML。"""
import os, sys, json
from spire.presentation import *
from spire.presentation.common import *

OUT = "/tmp/spire_probe/out"
os.makedirs(OUT, exist_ok=True)

RECT = RectangleF.FromLTRB(60.0, 40.0, 660.0, 120.0)


def new_deck(ratio_type=SlideSizeType.Screen16x9):
    p = Presentation()
    while p.Slides.Count > 0:
        p.Slides.RemoveAt(0)
    p.SlideSize.Type = ratio_type
    return p


# ---------- 1. 文字框 ----------
def probe_text_shape():
    p = new_deck()
    slide = p.Slides.Append()
    shape = slide.Shapes.AppendShape(ShapeType.Rectangle, RECT)
    shape.Name = "Block_text"
    shape.TextFrame.Text = "Hello MinerU"
    shape.TextFrame.FitTextToShape = True
    shape.TextFrame.MarginLeft = 0
    shape.TextFrame.MarginRight = 0
    shape.TextFrame.MarginTop = 0
    shape.TextFrame.MarginBottom = 0
    shape.Line.FillType = FillFormatType.none
    shape.Fill.FillType = FillFormatType.none
    for paragraph in shape.TextFrame.Paragraphs:
        paragraph.Alignment = TextAlignmentType.Center
        for tr in paragraph.TextRanges:
            tr.LatinFont = TextFont("Calibri")
            tr.FontHeight = 24.0
            tr.Fill.FillType = FillFormatType.Solid
            tr.Fill.SolidColor.Color = Color.FromArgb(255, 0, 0, 0)
    p.SaveToFile(f"{OUT}/1_text.pptx", FileFormat.Pptx2016)
    p.Dispose()


# ---------- 2. 背景圖片 ----------
def probe_background(png):
    p = new_deck()
    slide = p.Slides.Append()
    bg = slide.SlideBackground
    bg.Type = BackgroundType.Custom
    bg.Fill.FillType = FillFormatType.Picture
    stream = Stream(png)
    img = p.Images.AppendStream(stream)
    bg.Fill.PictureFill.Picture.EmbedImage = img
    bg.Fill.PictureFill.FillType = PictureFillType.Stretch
    p.SaveToFile(f"{OUT}/2_background.pptx", FileFormat.Pptx2016)
    p.Dispose()


# ---------- 3. 插圖置底 ----------
def probe_picture(png):
    p = new_deck()
    slide = p.Slides.Append()
    img_shape = slide.Shapes.AppendEmbedImageByPath(ShapeType.Rectangle, png, RECT)
    img_shape.Line.FillType = FillFormatType.none
    img_shape.ZOrderPosition = 0
    p.SaveToFile(f"{OUT}/3_picture.pptx", FileFormat.Pptx2016)
    p.Dispose()


# ---------- 4a. 單頁來源檔（模擬 NotebookLM 產出的單頁 pptx） ----------
def make_single_page(path, text):
    p = new_deck()
    slide = p.Slides.Append()
    shape = slide.Shapes.AppendShape(ShapeType.Rectangle, RECT)
    shape.Name = "New shape"
    shape.TextFrame.Text = text
    shape.Fill.FillType = FillFormatType.none
    p.SaveToFile(path, FileFormat.Pptx2016)
    p.Dispose()


# ---------- 4b. 合併（combine_ppt_files_with_spire 的等價流程） ----------
def probe_merge(sources, out):
    main = Presentation()
    main.LoadFromFile(sources[0])
    while main.Slides.Count > 1:
        main.Slides.RemoveAt(1)
    for src in sources[1:]:
        tmp = Presentation()
        tmp.LoadFromFile(src)
        main.Slides.AppendBySlide(tmp.Slides[0])
        tmp.Dispose()
    main.SaveToFile(out, FileFormat.Pptx2016)
    main.Dispose()


# ---------- 5. clean_ppt 等價（刪 name == "New shape" 的形狀） ----------
def probe_remove_shape(src, out):
    p = Presentation()
    p.LoadFromFile(src)
    for slide in p.Slides:
        for i in range(slide.Shapes.Count - 1, -1, -1):
            if slide.Shapes[i].Name == "New shape":
                slide.Shapes.RemoveAt(i)
    p.SaveToFile(out, FileFormat.Pptx2016)
    p.Dispose()


if __name__ == "__main__":
    png = sys.argv[1]
    probe_text_shape()
    print("1_text ok")
    probe_background(png)
    print("2_background ok")
    probe_picture(png)
    print("3_picture ok")
    make_single_page(f"{OUT}/src_a.pptx", "Page A body")
    make_single_page(f"{OUT}/src_b.pptx", "Page B body")
    make_single_page(f"{OUT}/src_c.pptx", "Page C body")
    print("src_* ok")
    probe_merge([f"{OUT}/src_a.pptx", f"{OUT}/src_b.pptx", f"{OUT}/src_c.pptx"],
                f"{OUT}/4_merged.pptx")
    print("4_merged ok")
    probe_remove_shape(f"{OUT}/4_merged.pptx", f"{OUT}/5_cleaned.pptx")
    print("5_cleaned ok")
