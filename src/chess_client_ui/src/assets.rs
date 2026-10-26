//! Встроенные PNG-ассеты фигур.
//!
//! Логика:
//!   - Орёл (N2), Гепард (R2), Джинн (J), Пешка (P) — программная обводка
//!     (только для слота запаса; на доске без неё):
//!       белые → чёрная (0,0,0), чёрные → белая (255,255,255);
//!       Гепард — толщина 2, остальные — 1.
//!   - K, Q, R, B, N — без программной обводки: контур уже в PNG.
//!   - Prince (P2) — King × 0.7, Princess (P3) — Queen × 0.7, без обводки.
//!   - «Своя» фигура (owner == my_seat) — жёлтый контур 2px.
//!
//! Храним:
//!   - `pixmaps`       — с обводкой (для слота запаса);
//!   - `pixmaps_plain` — без обводки (для доски, drag, промоушена);
//!   - `pixmaps_mine`  — с жёлтым контуром (для своих фигур).

use chess_logic::{Color, PieceType};
use tiny_skia::{Pixmap, PremultipliedColorU8};

struct RawPng {
    color: Color,
    piece_type: PieceType,
    bytes: &'static [u8],
}

const RAW_PNGS: &[RawPng] = &[
    // Белые.
    RawPng { color: Color::White, piece_type: PieceType::King,     bytes: include_bytes!("../assets/pieces/wK.png") },
    RawPng { color: Color::White, piece_type: PieceType::Queen,    bytes: include_bytes!("../assets/pieces/wQ.png") },
    RawPng { color: Color::White, piece_type: PieceType::Rook,     bytes: include_bytes!("../assets/pieces/wR.png") },
    RawPng { color: Color::White, piece_type: PieceType::Bishop,   bytes: include_bytes!("../assets/pieces/wB.png") },
    RawPng { color: Color::White, piece_type: PieceType::Knight,   bytes: include_bytes!("../assets/pieces/wN.png") },
    RawPng { color: Color::White, piece_type: PieceType::Pawn,     bytes: include_bytes!("../assets/pieces/wP.png") },
    RawPng { color: Color::White, piece_type: PieceType::Eagle,    bytes: include_bytes!("../assets/pieces/wN2.png") },
    RawPng { color: Color::White, piece_type: PieceType::Cheetah,  bytes: include_bytes!("../assets/pieces/wR2.png") },
    RawPng { color: Color::White, piece_type: PieceType::Genie,    bytes: include_bytes!("../assets/pieces/wJ.png") },
    // Royal'ы (белые).
    RawPng { color: Color::White, piece_type: PieceType::Prince,   bytes: include_bytes!("../assets/pieces/wPrn.png") },
    RawPng { color: Color::White, piece_type: PieceType::Princess, bytes: include_bytes!("../assets/pieces/wPrc.png") },

    // Чёрные.
    RawPng { color: Color::Black, piece_type: PieceType::King,     bytes: include_bytes!("../assets/pieces/bK.png") },
    RawPng { color: Color::Black, piece_type: PieceType::Queen,    bytes: include_bytes!("../assets/pieces/bQ.png") },
    RawPng { color: Color::Black, piece_type: PieceType::Rook,     bytes: include_bytes!("../assets/pieces/bR.png") },
    RawPng { color: Color::Black, piece_type: PieceType::Bishop,   bytes: include_bytes!("../assets/pieces/bB.png") },
    RawPng { color: Color::Black, piece_type: PieceType::Knight,   bytes: include_bytes!("../assets/pieces/bN.png") },
    RawPng { color: Color::Black, piece_type: PieceType::Pawn,     bytes: include_bytes!("../assets/pieces/bP.png") },
    RawPng { color: Color::Black, piece_type: PieceType::Eagle,    bytes: include_bytes!("../assets/pieces/bN2.png") },
    RawPng { color: Color::Black, piece_type: PieceType::Cheetah,  bytes: include_bytes!("../assets/pieces/bR2.png") },
    RawPng { color: Color::Black, piece_type: PieceType::Genie,    bytes: include_bytes!("../assets/pieces/bJ.png") },
    // Royal'ы (чёрные).
    RawPng { color: Color::Black, piece_type: PieceType::Prince,   bytes: include_bytes!("../assets/pieces/bPrn.png") },
    RawPng { color: Color::Black, piece_type: PieceType::Princess, bytes: include_bytes!("../assets/pieces/bPrc.png") },
];

const MY_OUTLINE_COLOR: [u8; 3] = [0, 255, 0]; // Зелёный
const MY_OUTLINE_THICKNESS: u32 = 2;

pub struct PieceAtlas {
    pub cell: u32,
    pixmaps: Vec<(Color, PieceType, Pixmap)>,
    pixmaps_plain: Vec<(Color, PieceType, Pixmap)>,
    pixmaps_mine: Vec<(Color, PieceType, Pixmap)>,
}

impl PieceAtlas {
    pub fn build(cell: u32) -> Self {
        let cell = cell.max(1);
        let mut pixmaps = Vec::with_capacity(RAW_PNGS.len() + 4);
        let mut pixmaps_plain = Vec::with_capacity(RAW_PNGS.len() + 4);
        let mut pixmaps_mine = Vec::with_capacity(RAW_PNGS.len() + 4);

        for raw in RAW_PNGS {
            // С обводкой (для слота).
            let outline = outline_params(raw.color, raw.piece_type);
            match decode_and_scale(raw.bytes, cell, outline) {
                Ok(pix) => pixmaps.push((raw.color, raw.piece_type, pix)),
                Err(e) => tracing::warn!("[assets] с обводкой {} {:?}: {e}", raw.color.prefix(), raw.piece_type),
            }

            // Без обводки.
            match decode_and_scale(raw.bytes, cell, None) {
                Ok(pix) => {
                    // С жёлтым контуром — из того же plain pixmap.
                    // Больше не используем зелёный контур.
                    pixmaps_mine.push((raw.color, raw.piece_type, pix.clone()));
                    pixmaps_plain.push((raw.color, raw.piece_type, pix));
                }
                Err(e) => tracing::warn!("[assets] без обводки {} {:?}: {e}", raw.color.prefix(), raw.piece_type),
            }
        }



        tracing::info!(
            "[assets] {} с обводкой / {} без обводки / {} моих (cell={})",
                       pixmaps.len(), pixmaps_plain.len(), pixmaps_mine.len(), cell
        );

        Self { cell, pixmaps, pixmaps_plain, pixmaps_mine }
    }

    pub fn len(&self) -> usize { self.pixmaps_plain.len() }
    pub fn is_empty(&self) -> bool { self.pixmaps_plain.is_empty() }

    /// С обводкой (для слота запаса).
    pub fn get(&self, color: Color, piece_type: PieceType) -> Option<&Pixmap> {
        self.pixmaps.iter()
        .find(|(c, t, _)| *c == color && *t == piece_type)
        .map(|(_, _, p)| p)
    }

    /// Без обводки (для доски, drag, промоушена).
    pub fn get_plain(&self, color: Color, piece_type: PieceType) -> Option<&Pixmap> {
        self.pixmaps_plain.iter()
        .find(|(c, t, _)| *c == color && *t == piece_type)
        .map(|(_, _, p)| p)
    }

    /// С жёлтым контуром (для своих фигур).
    pub fn get_mine(&self, color: Color, piece_type: PieceType) -> Option<&Pixmap> {
        self.pixmaps_mine.iter()
        .find(|(c, t, _)| *c == color && *t == piece_type)
        .map(|(_, _, p)| p)
    }
}

fn find_bytes(color: Color, pt: PieceType) -> Option<&'static [u8]> {
    RAW_PNGS.iter()
    .find(|r| r.color == color && r.piece_type == pt)
    .map(|r| r.bytes)
}

fn outline_params(color: Color, piece_type: PieceType) -> Option<([u8; 3], u32)> {
    // Для чёрных фигур обводку не рисуем вообще.
    if color == Color::Black {
        return None;
    }

    let color_rgb: [u8; 3] = match color {
        Color::White => [0, 0, 0],
        Color::Black => [255, 255, 255], // (эта ветка теперь недостижима)
    };

    match piece_type {
        PieceType::Cheetah => Some((color_rgb, 2)),
        PieceType::Eagle => Some((color_rgb, 1)),
        PieceType::Genie => Some((color_rgb, 1)),
        PieceType::Pawn => Some((color_rgb, 1)),
        _ => None,
    }
}

fn decode_and_scale(
    png_bytes: &[u8],
    size: u32,
    outline: Option<([u8; 3], u32)>,
) -> anyhow::Result<Pixmap> {
    use anyhow::anyhow;
    let img = image::load_from_memory(png_bytes)
    .map_err(|e| anyhow!("decode png: {e}"))?
    .to_rgba8();

    let thickness = outline.map_or(0, |(_, t)| t);
    let inner_size = size.saturating_sub(2 * thickness).max(1);

    let (w, h) = img.dimensions();
    let scale = (inner_size as f32 / w as f32).min(inner_size as f32 / h as f32);
    let new_w = ((w as f32 * scale).round() as u32).max(1);
    let new_h = ((h as f32 * scale).round() as u32).max(1);

    let scaled = if new_w == w && new_h == h {
        img
    } else {
        image::imageops::resize(&img, new_w, new_h, image::imageops::FilterType::Lanczos3)
    };

    let scaled = match outline {
        Some((color, t)) => add_outline_to_rgba(&scaled, color, t),
        None => scaled,
    };

    let (sw, sh) = scaled.dimensions();
    let mut pix = Pixmap::new(size, size)
    .ok_or_else(|| anyhow!("Pixmap::new({size},{size})"))?;

    let offset_x = (size as i32 - sw as i32) / 2;
    let offset_y = (size as i32 - sh as i32) / 2;

    let pmut = pix.pixels_mut();
    let pix_w = size as usize;

    for (x, y, pixel) in scaled.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        if a == 0 { continue; }
        let r = ((r as u16 * a as u16) / 255) as u8;
        let g = ((g as u16 * a as u16) / 255) as u8;
        let b = ((b as u16 * a as u16) / 255) as u8;
        let px = match PremultipliedColorU8::from_rgba(r, g, b, a) {
            Some(p) => p,
            None => continue,
        };
        let tx = offset_x + x as i32;
        let ty = offset_y + y as i32;
        if tx < 0 || ty < 0 || tx >= size as i32 || ty >= size as i32 { continue; }
        let idx = (ty as usize) * pix_w + (tx as usize);
        if idx < pmut.len() { pmut[idx] = px; }
    }
    Ok(pix)
}

/// Уменьшить существующий Pixmap до `scale` и вернуть новый Pixmap
/// того же размера, с масштабированной фигурой по центру.
/// Уменьшить Pixmap до `scale` и вернуть уменьшенный Pixmap (без возврата к исходному размеру).
fn scaled_down_pixmap(src: &Pixmap, scale: f32) -> anyhow::Result<Pixmap> {
    use anyhow::anyhow;
    let w = src.width() as f32;
    let h = src.height() as f32;
    let new_w = ((w * scale).round() as u32).max(1);
    let new_h = ((h * scale).round() as u32).max(1);
    println!("[scaled_down] src={}x{}, new={}x{}", src.width(), src.height(), new_w, new_h);

    // Конвертируем в RgbaImage.
    let mut img = image::RgbaImage::new(src.width(), src.height());
    for y in 0..src.height() {
        for x in 0..src.width() {
            let c = src.pixel(x, y).unwrap_or_else(|| PremultipliedColorU8::from_rgba(0,0,0,0).unwrap());
            let a = c.alpha();
            let r = if a == 0 { 0 } else { (c.red() as u32 * 255 / a as u32).min(255) as u8 };
            let g = if a == 0 { 0 } else { (c.green() as u32 * 255 / a as u32).min(255) as u8 };
            let b = if a == 0 { 0 } else { (c.blue() as u32 * 255 / a as u32).min(255) as u8 };
            img.put_pixel(x, y, image::Rgba([r, g, b, a]));
        }
    }

    // Уменьшаем до нужного размера (без центрирования в исходном).
    let small = image::imageops::resize(&img, new_w, new_h, image::imageops::FilterType::Lanczos3);

    // Создаём Pixmap нового размера.
    let mut out = Pixmap::new(new_w, new_h).ok_or_else(|| anyhow!("Pixmap::new"))?;
    let pmut = out.pixels_mut();
    let pw = new_w as usize;
    for (x, y, p) in small.enumerate_pixels() {
        let [r, g, b, a] = p.0;
        if a == 0 { continue; }
        let pr = ((r as u16 * a as u16) / 255) as u8;
        let pg = ((g as u16 * a as u16) / 255) as u8;
        let pb = ((b as u16 * a as u16) / 255) as u8;
        if let Some(px) = PremultipliedColorU8::from_rgba(pr, pg, pb, a) {
            let idx = (y as usize) * pw + (x as usize);
            if idx < pmut.len() { pmut[idx] = px; }
        }
    }
    Ok(out)
}

fn add_outline_to_pixmap(
    src: &Pixmap,
    color: [u8; 3],
    thickness: u32,
) -> anyhow::Result<Pixmap> {
    use anyhow::anyhow;
    let w = src.width();
    let h = src.height();
    let mut img = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let c = src.pixel(x, y).unwrap_or_else(|| PremultipliedColorU8::from_rgba(0,0,0,0).unwrap());
            let a = c.alpha();
            let r = if a == 0 { 0 } else { (c.red() as u32 * 255 / a as u32).min(255) as u8 };
            let g = if a == 0 { 0 } else { (c.green() as u32 * 255 / a as u32).min(255) as u8 };
            let b = if a == 0 { 0 } else { (c.blue() as u32 * 255 / a as u32).min(255) as u8 };
            img.put_pixel(x, y, image::Rgba([r, g, b, a]));
        }
    }

    let outlined = add_outline_to_rgba(&img, color, thickness);
    let mut out = Pixmap::new(w, h).ok_or_else(|| anyhow!("Pixmap::new"))?;
    let pmut = out.pixels_mut();
    let pw = w as usize;
    for (x, y, p) in outlined.enumerate_pixels() {
        let [r, g, b, a] = p.0;
        if a == 0 { continue; }
        let pr = ((r as u16 * a as u16) / 255) as u8;
        let pg = ((g as u16 * a as u16) / 255) as u8;
        let pb = ((b as u16 * a as u16) / 255) as u8;
        if let Some(px) = PremultipliedColorU8::from_rgba(pr, pg, pb, a) {
            let idx = (y as usize) * pw + (x as usize);
            if idx < pmut.len() { pmut[idx] = px; }
        }
    }
    Ok(out)
}

fn add_outline_to_rgba(
    img: &image::RgbaImage,
    outline_color: [u8; 3],
    thickness: u32,
) -> image::RgbaImage {
    let (w, h) = img.dimensions();
    let pad = thickness;
    let out_w = w + 2 * pad;
    let out_h = h + 2 * pad;
    let mut out = image::RgbaImage::new(out_w, out_h);

    // Контур.
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            if p.0[3] == 0 { continue; }
            let cx = x + pad;
            let cy = y + pad;
            let x0 = cx.saturating_sub(pad);
            let y0 = cy.saturating_sub(pad);
            let x1 = (cx + pad).min(out_w - 1);
            let y1 = (cy + pad).min(out_h - 1);
            for ny in y0..=y1 {
                for nx in x0..=x1 {
                    let out_px = out.get_pixel_mut(nx, ny);
                    if out_px.0[3] == 0 {
                        *out_px = image::Rgba([
                            outline_color[0],
                            outline_color[1],
                            outline_color[2],
                            255,
                        ]);
                    }
                }
            }
        }
    }
    // Оригинал поверх.
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            if p.0[3] == 0 { continue; }
            out.put_pixel(x + pad, y + pad, *p);
        }
    }
    out
}
