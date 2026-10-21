"""修正版：來源檔形狀用非 "New shape" 名稱，驗證 clean_ppt 只會刪掉 Spire 的評估水印。"""
import os
from spire.presentation import *
from spire.presentation.common import *
OUT="/tmp/spire_probe/out"
RECT = RectangleF.FromLTRB(60.0, 40.0, 660.0, 120.0)

def make(path, text):
    p = Presentation()
    while p.Slides.Count > 0: p.Slides.RemoveAt(0)
    p.SlideSize.Type = SlideSizeType.Screen16x9
    s = p.Slides.Append()
    sh = s.Shapes.AppendShape(ShapeType.Rectangle, RECT)
    sh.Name = "Block_text"          # 不與水印撞名
    sh.TextFrame.Text = text
    p.SaveToFile(path, FileFormat.Pptx2016); p.Dispose()

def merge(srcs, out):
    m = Presentation(); m.LoadFromFile(srcs[0])
    while m.Slides.Count > 1: m.Slides.RemoveAt(1)
    for s in srcs[1:]:
        t = Presentation(); t.LoadFromFile(s)
        m.Slides.AppendBySlide(t.Slides[0]); t.Dispose()
    m.SaveToFile(out, FileFormat.Pptx2016); m.Dispose()

def clean(src, out):
    p = Presentation(); p.LoadFromFile(src)
    removed = 0
    for slide in p.Slides:
        for i in range(slide.Shapes.Count-1, -1, -1):
            if slide.Shapes[i].Name == "New shape":
                slide.Shapes.RemoveAt(i); removed += 1
    p.SaveToFile(out, FileFormat.Pptx2016); p.Dispose()
    return removed

for n,t in (("a","Page A body"),("b","Page B body"),("c","Page C body")):
    make(f"{OUT}/fix_{n}.pptx", t)
merge([f"{OUT}/fix_{n}.pptx" for n in "abc"], f"{OUT}/fix_merged.pptx")
print("removed watermark shapes:", clean(f"{OUT}/fix_merged.pptx", f"{OUT}/fix_cleaned.pptx"))
