//! Apple Books Exporter - Card Generator with Font Rendering
//! Generates PNG images from notes/highlights with real text rendering

use image::{ImageBuffer, Rgba};
use rusttype::{Font, Point, Scale};
use std::path::{Path, PathBuf};

/// 卡片样式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CardStyle {
    Dark,
    Light,
    Minimal,
}

impl CardStyle {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "light" | "minimal" => CardStyle::Minimal,
            _ => CardStyle::Dark,
        }
    }

    /// 背景色
    pub fn bg_color(&self) -> Rgba<u8> {
        match self {
            CardStyle::Dark => Rgba([30, 30, 46, 255]),
            CardStyle::Light => Rgba([245, 244, 238, 255]),
            CardStyle::Minimal => Rgba([255, 255, 255, 255]),
        }
    }

    /// 文字颜色
    pub fn text_color(&self) -> Rgba<u8> {
        match self {
            CardStyle::Dark => Rgba([220, 220, 230, 255]),
            CardStyle::Light => Rgba([50, 50, 60, 255]),
            CardStyle::Minimal => Rgba([40, 40, 50, 255]),
        }
    }

    /// 高亮颜色
    pub fn accent_color(&self) -> Rgba<u8> {
        match self {
            CardStyle::Dark => Rgba([139, 233, 253, 255]),
            CardStyle::Light | CardStyle::Minimal => Rgba([100, 150, 200, 255]),
        }
    }

    /// 边框颜色
    pub fn border_color(&self) -> Rgba<u8> {
        match self {
            CardStyle::Dark => Rgba([80, 80, 100, 255]),
            CardStyle::Light => Rgba([200, 200, 210, 255]),
            CardStyle::Minimal => Rgba([230, 230, 235, 255]),
        }
    }
}

/// 字体管理器
pub struct FontManager {
    font: Font<'static>,
    scale: Scale,
}

impl FontManager {
    /// 创建字体管理器
    pub fn new(font_path: &str, size: f32) -> anyhow::Result<Self> {
        let font_data = std::fs::read(font_path).or_else(|_| {
            let mut paths = vec![
                PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
                PathBuf::from("/System/Library/Fonts/STHeiti Medium.ttc"),
                PathBuf::from("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc"),
                PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
            ];

            if let Some(home) = crate::utils::home_dir() {
                paths.push(home.join("Library/Fonts/NotoSansSC-R.ttf"));
            }

            for p in paths {
                if let Ok(data) = std::fs::read(p) {
                    return Ok(data);
                }
            }
            Err(anyhow::anyhow!("未找到可用字体"))
        })?;

        let font =
            Font::try_from_vec(font_data).ok_or_else(|| anyhow::anyhow!("无法加载字体文件"))?;
        let scale = Scale::uniform(size);

        Ok(Self { font, scale })
    }

    /// 文本换行
    fn wrap_text(&self, text: &str, max_width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut current = String::new();
        let mut current_width = 0.0;

        for ch in text.chars() {
            let glyph = self.font.glyph(ch).scaled(self.scale);
            let metrics = glyph.h_metrics();
            let advance = metrics.advance_width;

            if current_width + advance > max_width && !current.is_empty() {
                lines.push(current.clone());
                current.clear();
                current_width = 0.0;
            }

            current.push(ch);
            current_width += advance;
        }

        if !current.is_empty() {
            lines.push(current);
        }

        if lines.is_empty() {
            lines.push(text.to_string());
        }

        lines
    }

    /// 绘制文本到图片
    fn draw_text(
        &self,
        img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
        text: &str,
        x: f32,
        mut y: f32,
        color: Rgba<u8>,
    ) {
        for line in self.wrap_text(text, img.width() as f32) {
            self.draw_line(img, &line, x, y, color);
            y += self.scale.y;
        }
    }

    /// 绘制单行文本
    fn draw_line(
        &self,
        img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
        text: &str,
        x: f32,
        y: f32,
        color: Rgba<u8>,
    ) {
        let glyphs: Vec<_> = self.font.layout(text, self.scale, Point { x, y }).collect();

        for glyph in glyphs {
            if let Some(bb) = glyph.pixel_bounding_box() {
                glyph.draw(|gx, gy, c| {
                    let px = bb.min.x + gx as i32;
                    let py = bb.min.y + gy as i32;

                    if px >= 0 && px < img.width() as i32 && py >= 0 && py < img.height() as i32 {
                        let alpha = (c * 255.0) as u8;
                        let pixel = img.get_pixel_mut(px as u32, py as u32);
                        *pixel = Rgba([color[0], color[1], color[2], alpha]);
                    }
                });
            }
        }
    }
}

/// 截断文本到指定行数
fn truncate_to_lines(
    text: &str,
    font_manager: &FontManager,
    max_width: f32,
    max_lines: usize,
) -> String {
    let lines = font_manager.wrap_text(text, max_width);
    if lines.len() <= max_lines {
        text.to_string()
    } else {
        let truncated: String = lines[..max_lines].join("\n");
        format!("{}...", &truncated[..truncated.len().saturating_sub(3)])
    }
}

/// 生成卡片图片
pub fn generate_card(
    highlight: &str,
    explanation: Option<&str>,
    book_title: &str,
    style: CardStyle,
    output_path: &Path,
) -> anyhow::Result<()> {
    let width: u32 = 800;
    let padding: u32 = 60;
    let font_size: f32 = 28.0;
    let font_path = if cfg!(target_os = "macos") {
        "/System/Library/Fonts/PingFang.ttc"
    } else if cfg!(target_os = "linux") {
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc"
    } else {
        "NotoSansSC-R.ttf"
    };

    let font_manager = FontManager::new(font_path, font_size)?;
    let text_width = (width - padding * 2) as f32;

    // 截断高亮文本（最多 8 行）
    let highlight_truncated = truncate_to_lines(highlight, &font_manager, text_width, 8);
    let highlight_lines = font_manager.wrap_text(&highlight_truncated, text_width).len() as f32;
    let highlight_height = highlight_lines * font_manager.scale.y;

    // 截断解释文本（最多 10 行）
    let exp_truncated =
        explanation.map(|exp| truncate_to_lines(exp, &font_manager, text_width, 10));
    let exp_height = exp_truncated
        .as_ref()
        .map(|e| font_manager.wrap_text(e, text_width).len() as f32 * font_manager.scale.y)
        .unwrap_or(0.0);

    // 动态计算卡片高度
    let min_height = 400;
    let content_height =
        (padding as f32 * 2.0) + 40.0 + highlight_height + 30.0 + exp_height + 60.0;
    let height = (content_height as u32).max(min_height);

    let mut img = ImageBuffer::new(width, height);

    // 填充背景色
    fill_rect(&mut img, 0, 0, width, height, style.bg_color());

    // 绘制边框
    draw_border(&mut img, style.border_color());

    // 绘制顶部装饰线
    draw_accent_line(&mut img, style.accent_color());

    // 计算文本区域
    let text_x = padding as f32;
    let text_y = (padding + 40) as f32;

    // 绘制高亮内容（主要文字）
    font_manager.draw_text(
        &mut img,
        &highlight_truncated,
        text_x,
        text_y,
        style.text_color(),
    );

    // 绘制解释（如果有）
    if let Some(exp) = &exp_truncated {
        let exp_y = text_y + highlight_height + 30.0;

        // 绘制分隔线
        draw_horizontal_line(
            &mut img,
            padding,
            exp_y as u32 - 10,
            text_width as u32,
            style.border_color(),
        );

        font_manager.draw_text(&mut img, exp, text_x, exp_y, style.accent_color());
    }

    // 绘制底部信息
    let footer_y = (height - padding as u32 - 30) as f32;
    draw_footer(&mut img, book_title, footer_y, style, &font_manager);

    // 保存图片
    img.save(output_path)?;

    Ok(())
}

/// 填充矩形区域
fn fill_rect(
    img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color: Rgba<u8>,
) {
    for i in x..x.saturating_add(w) {
        for j in y..y.saturating_add(h) {
            *img.get_pixel_mut(i, j) = color;
        }
    }
}

/// 绘制边框
fn draw_border(img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, color: Rgba<u8>) {
    let (width, height) = img.dimensions();
    let border_width = 4;

    fill_rect(img, 0, 0, width, border_width, color);
    fill_rect(img, 0, height - border_width, width, border_width, color);
    fill_rect(img, 0, 0, border_width, height, color);
    fill_rect(img, width - border_width, 0, border_width, height, color);
}

/// 绘制顶部装饰线
fn draw_accent_line(img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, color: Rgba<u8>) {
    let (width, _) = img.dimensions();
    fill_rect(img, 60, 60, width - 120, 6, color);
}

/// 绘制水平线
fn draw_horizontal_line(
    img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    x: u32,
    y: u32,
    w: u32,
    color: Rgba<u8>,
) {
    fill_rect(img, x, y, w, 2, color);
}

/// 绘制页脚
fn draw_footer(
    img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    book_title: &str,
    y: f32,
    style: CardStyle,
    font_manager: &FontManager,
) {
    let width = img.width();

    // 分隔线
    draw_horizontal_line(
        img,
        60,
        (y - 25.0) as u32,
        width - 120,
        style.border_color(),
    );

    // 书名
    let footer_text = format!("[ {} ]", book_title);
    font_manager.draw_text(img, &footer_text, 60.0, y, style.text_color());
}

/// 生成卡片文件名
pub fn card_filename(highlight: &str, index: usize) -> String {
    let safe = crate::utils::sanitize_filename(highlight);
    format!("card_{:02}_{}.png", index, safe)
}