"""生成 GalManager 应用图标（程序化绘制，无需外部素材）。

输出 1024x1024 PNG，供 `tauri icon` 生成各平台图标集。
设计语言与前端一致：紫罗兰 → 玫瑰粉 渐变 + 圆角方形 + 白色 G 字标。
"""

from __future__ import annotations

import os
from PIL import Image, ImageDraw, ImageFilter, ImageFont

SIZE = 1024
RADIUS = int(SIZE * 0.225)
OUT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "assets")

# 与前端 --color-accent / --color-rose 保持一致
COLOR_FROM = (139, 124, 246)
COLOR_TO = (244, 114, 182)
COLOR_GLOW = (167, 155, 250)

FONT_CANDIDATES = [
    r"C:\Windows\Fonts\arialbd.ttf",
    r"C:\Windows\Fonts\segoeuib.ttf",
    r"C:\Windows\Fonts\msyhbd.ttc",
    r"C:\Windows\Fonts\segoeui.ttf",
]


def diagonal_gradient(size: int) -> Image.Image:
    """左上 → 右下 的对角线性渐变"""
    gradient = Image.new("RGB", (size, size))
    pixels = gradient.load()
    for y in range(size):
        for x in range(size):
            t = (x + y) / (2 * (size - 1))
            pixels[x, y] = (
                round(COLOR_FROM[0] + (COLOR_TO[0] - COLOR_FROM[0]) * t),
                round(COLOR_FROM[1] + (COLOR_TO[1] - COLOR_FROM[1]) * t),
                round(COLOR_FROM[2] + (COLOR_TO[2] - COLOR_FROM[2]) * t),
            )
    return gradient


def rounded_mask(size: int, radius: int) -> Image.Image:
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=255)
    return mask


def load_font(size: int) -> ImageFont.FreeTypeFont:
    for path in FONT_CANDIDATES:
        if os.path.exists(path):
            try:
                return ImageFont.truetype(path, size)
            except OSError:
                continue
    return ImageFont.load_default()


def draw_glyph(size: int) -> Image.Image:
    """白色字母 G + 柔光"""
    layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)

    font = load_font(int(size * 0.60))
    text = "G"
    box = draw.textbbox((0, 0), text, font=font)
    text_w = box[2] - box[0]
    text_h = box[3] - box[1]
    position = ((size - text_w) / 2 - box[0], (size - text_h) / 2 - box[1] - size * 0.015)

    # 先画一层柔光，让字标有悬浮感
    glow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    ImageDraw.Draw(glow).text(position, text, font=font, fill=(255, 255, 255, 130))
    glow = glow.filter(ImageFilter.GaussianBlur(size * 0.022))
    layer = Image.alpha_composite(layer, glow)

    draw = ImageDraw.Draw(layer)
    draw.text(position, text, font=font, fill=(255, 255, 255, 255))
    return layer


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)

    canvas = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))

    # 背景：外发光（让图标在深色任务栏上也有轮廓）
    glow_layer = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    ImageDraw.Draw(glow_layer).rounded_rectangle(
        (SIZE * 0.045, SIZE * 0.045, SIZE * 0.955, SIZE * 0.955),
        radius=RADIUS,
        fill=COLOR_GLOW + (110,),
    )
    glow_layer = glow_layer.filter(ImageFilter.GaussianBlur(SIZE * 0.035))
    canvas = Image.alpha_composite(canvas, glow_layer)

    # 主体渐变方块
    body = diagonal_gradient(SIZE).convert("RGBA")
    mask = rounded_mask(SIZE, RADIUS)
    canvas.paste(body, (0, 0), mask)

    # 顶部高光，增强玻璃质感
    highlight = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    ImageDraw.Draw(highlight).rounded_rectangle(
        (0, 0, SIZE - 1, int(SIZE * 0.52)),
        radius=RADIUS,
        fill=(255, 255, 255, 34),
    )
    highlight = highlight.filter(ImageFilter.GaussianBlur(SIZE * 0.02))
    canvas = Image.alpha_composite(canvas, Image.composite(
        highlight, Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0)), mask
    ))

    # 字标
    canvas = Image.alpha_composite(canvas, draw_glyph(SIZE))

    # 重新裁一次圆角，去掉发光溢出的部分
    final = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    final.paste(canvas, (0, 0), mask)

    out_path = os.path.join(OUT_DIR, "icon.png")
    final.save(out_path, "PNG")
    print(f"图标已生成: {out_path} ({SIZE}x{SIZE})")

    # 顺带输出一张 512 的方形图，供 README 使用
    final.resize((512, 512), Image.LANCZOS).save(
        os.path.join(OUT_DIR, "icon-512.png"), "PNG"
    )


if __name__ == "__main__":
    main()
