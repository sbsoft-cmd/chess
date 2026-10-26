//! Рендер текста через fontdue + tiny-skia.
//!
//! Глифы сажаются по БАЗОВОЙ ЛИНИИ: используется metrics.ymin,
//! чтобы буквы и цифры с разной высотой (б, у, д, 1, 2, …) стояли
//! на одной строке, а не «прыгали».

use fontdue::{Font, FontSettings};
use tiny_skia::{Color, Pixmap, PremultipliedColorU8};

/// Растрированный глиф в кэше.
#[derive(Clone)]
struct Glyph {
    width: usize,
    height: usize,
    /// ymin из metrics: смещение нижнего края bitmap-а от базовой линии.
    /// В fontdue положительное значение означает, что bitmap поднят выше
    /// базовой линии.
    ymin: i32,
    bitmap: Vec<u8>,
}

/// Обёртка над шрифтом с кэшем растрированных глифов.
pub struct TextRenderer {
    font: Font,
    cache: std::collections::HashMap<(char, u32), Glyph>,
}

impl TextRenderer {
    /// Загрузить шрифт из встроенных байтов.
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let font = Font::from_bytes(bytes, FontSettings::default())
        .map_err(|e| anyhow::anyhow!("fontdue: {e}"))?;
        Ok(Self {
            font,
            cache: std::collections::HashMap::new(),
        })
    }

    /// Нарисовать строку.
    ///
    /// `(x, y)` — координаты **базовой линии** первого символа.
    /// Все глифы в строке выравниваются по этой базовой линии.
    pub fn draw_text(
        &mut self,
        pixmap: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        size_px: f32,
        color: Color,
    ) {
        let (cr, cg, cb, ca) = {
            let c = color.to_color_u8();
            (c.red(), c.green(), c.blue(), c.alpha())
        };

        let mut cursor_x = x;
        let baseline_y = y.round() as i32;

        for ch in text.chars() {
            let size_key = size_px.round() as u32;
            let key = (ch, size_key);

            // Получаем (или растеризуем и кэшируем) глиф.
            let glyph = self
            .cache
            .entry(key)
            .or_insert_with(|| {
                let (metrics, bitmap) = self.font.rasterize(ch, size_px);
                Glyph {
                    width: metrics.width,
                    height: metrics.height,
                    ymin: metrics.ymin,
                    bitmap,
                }
            })
            .clone();

            // Верх bitmap-а на экране:
            //   baseline_y - (ymin + height)
            // Потому что ymin — смещение НИЖНЕГО края bitmap-а от базовой
            // линии (в fontdue положительное вверх), а сам bitmap растёт вверх
            // на height пикселей.
            let px_x = cursor_x.round() as i32;
            let px_y = baseline_y - (glyph.ymin + glyph.height as i32);

            // Сначала immutable-данные, потом mutable.
            let pix_w = pixmap.width() as i32;
            let pix_h = pixmap.height() as i32;
            let pmut = pixmap.pixels_mut();

            for gy in 0..glyph.height {
                for gx in 0..glyph.width {
                    let alpha = glyph.bitmap[gy * glyph.width + gx];
                    if alpha == 0 {
                        continue;
                    }

                    // Комбинируем альфу глифа с альфой цвета.
                    let final_a = ((alpha as u16 * ca as u16) / 255) as u8;
                    if final_a == 0 {
                        continue;
                    }
                    // Premultiply по цвету.
                    let rr = ((cr as u16 * final_a as u16) / 255) as u8;
                    let gg = ((cg as u16 * final_a as u16) / 255) as u8;
                    let bb = ((cb as u16 * final_a as u16) / 255) as u8;

                    let src = match PremultipliedColorU8::from_rgba(rr, gg, bb, final_a) {
                        Some(p) => p,
                        None => continue,
                    };

                    let tx = px_x + gx as i32;
                    let ty = px_y + gy as i32;

                    if tx < 0 || ty < 0 || tx >= pix_w || ty >= pix_h {
                        continue;
                    }

                    let idx = (ty as usize) * (pix_w as usize) + (tx as usize);
                    if idx < pmut.len() {
                        let dst = pmut[idx];
                        pmut[idx] = blend_over(src, dst);
                    }
                }
            }

            // Сдвиг курсора на advance_width.
            let advance = self.font.metrics(ch, size_px).advance_width;
            cursor_x += advance;
        }
    }

    /// Приблизительная ширина строки в пикселях.
    pub fn text_width(&self, text: &str, size_px: f32) -> f32 {
        let mut w = 0.0f32;
        for ch in text.chars() {
            w += self.font.metrics(ch, size_px).advance_width;
        }
        w
    }
}

/// SourceOver-смешивание premultiplied RGBA.
fn blend_over(
    src: PremultipliedColorU8,
    dst: PremultipliedColorU8,
) -> PremultipliedColorU8 {
    let sa = src.alpha() as u32;
    let da = dst.alpha() as u32;

    if sa == 0 {
        return dst;
    }
    if sa == 255 {
        return src;
    }

    let inv = 255 - sa;
    let a_out = sa + da * inv / 255;
    if a_out == 0 {
        return PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap();
    }

    let blend = |s: u8, d: u8| -> u8 {
        let s = s as u32;
        let d = d as u32;
        ((s * 255 + d * inv) / a_out) as u8
    };

    let r = blend(src.red(), dst.red());
    let g = blend(src.green(), dst.green());
    let b = blend(src.blue(), dst.blue());

    PremultipliedColorU8::from_rgba(r, g, b, a_out as u8)
    .unwrap_or_else(|| PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap())
}
