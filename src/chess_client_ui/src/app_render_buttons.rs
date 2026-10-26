//! Отрисовка круглых кнопок, tooltip и иконок (flip/resign/draw),
//! а также универсальный `draw_button_label` и `color_label`.

use crate::app::{
    App, ButtonKind, TOOLTIP_DELAY,
};
use crate::app::render_board::{fill_rect_rgba, outline_rect_rgba};
use crate::setup::Rect as SetupRect;
use chess_logic::Color;
use tiny_skia::{Color as SkColor, Paint, PathBuilder, Pixmap, Stroke, Transform};
use crate::app::ReviewAction;

impl App {
    // ============================================================
    // КНОПКИ
    // ============================================================

    pub(super) fn draw_buttons_bg(&mut self) {
        for b in &self.buttons {
            let is_hovered = self.hovered_button == Some(b.kind);
            let enabled = match b.kind {
                ButtonKind::Flip => true,
                ButtonKind::Resign | ButtonKind::Draw => self
                .state
                .as_ref()
                .map_or(false, |st| !st.is_over()),
            };

            let (fill, border, icon_color) = match (b.kind, enabled) {
                (ButtonKind::Flip, _) => ((60, 160, 200), (150, 220, 255), (255, 255, 255)),
                (ButtonKind::Resign, true) => ((220, 60, 60), (255, 150, 150), (255, 255, 255)),
                (ButtonKind::Resign, false) => ((90, 40, 40), (120, 70, 70), (160, 110, 110)),
                (ButtonKind::Draw, true) => ((255, 165, 0), (255, 200, 100), (255, 255, 255)),
                (ButtonKind::Draw, false) => ((90, 70, 40), (120, 100, 70), (160, 140, 110)),
            };

            let fill = if is_hovered { brighten(fill, 30) } else { fill };

            draw_circle_rgba(&mut self.bg_pixmap, b.cx, b.cy, b.radius, fill, 255);
            draw_circle_outline_rgba(&mut self.bg_pixmap, b.cx, b.cy, b.radius, border, 4.0);

            match b.kind {
                ButtonKind::Flip => draw_flip_icon(
                    &mut self.bg_pixmap, b.cx, b.cy, b.radius * 1.2, icon_color,
                ),
                ButtonKind::Resign => draw_flag_icon(
                    &mut self.bg_pixmap, b.cx, b.cy, b.radius * 1.2, icon_color,
                ),
                ButtonKind::Draw => draw_handshake_icon(
                    &mut self.bg_pixmap, b.cx, b.cy, b.radius * 1.2, icon_color,
                ),
            }
        }
    }

    pub(super) fn draw_review_buttons(&mut self) {
        if !self.state.as_ref().map_or(false, |st| st.is_over()) {
            return; // Кнопки только после конца игры.
        }
        if self.history.is_empty() { return; }

        let panel_cx = (crate::layout::LEFT_PANEL as f32) / 2.0;
        let size = 30.0;
        let gap = 6.0;
        let count = 4;
        let total_w = count as f32 * size + (count as f32 - 1.0) * gap;
        let start_x = panel_cx - total_w / 2.0;
        let y = self.height as f32 - 100.0; // Под кнопками

        // Рисуем кнопки и иконки.
        for (i, action) in [ReviewAction::Start, ReviewAction::Prev, ReviewAction::Next, ReviewAction::End].iter().enumerate() {
            let x = start_x + i as f32 * (size + gap);
            let rect = crate::setup::Rect { x, y, w: size, h: size };
            let active = self.review_index.is_some()
            || matches!(action, ReviewAction::Start | ReviewAction::End);

            fill_rect_rgba(&mut self.pixmap, x, y, size, size, 40, 40, 55, 255);
            outline_rect_rgba(&mut self.pixmap, x, y, size, size, (150, 150, 180), 2.0);

            // Рисуем стрелки (можно текстом или линиями).
            let label = match action {
                ReviewAction::Start => "⏮",
                ReviewAction::Prev => "◀",
                ReviewAction::Next => "▶",
                ReviewAction::End => "⏭",
            };
            // ... (отрисовка текста по центру).
        }
    }

    // ============================================================
    // TOOLTIP
    // ============================================================

    pub(super) fn draw_tooltip_if_needed(&mut self) {
        let Some(kind) = self.hovered_button else { return };
        let Some(since) = self.hovered_since else { return };
        if since.elapsed() < TOOLTIP_DELAY {
            return;
        }

        let label = match kind {
            ButtonKind::Flip => "Перевернуть доску",
            ButtonKind::Resign => "Сдаться",
            ButtonKind::Draw => "Предложить ничью",
        };

        let button = match self.buttons.iter().find(|b| b.kind == kind) {
            Some(b) => *b,
            None => return,
        };

        let face_size = 15.0;
        let text_w = self.text.text_width(label, face_size);
        let pad_x = 10.0;
        let pad_y = 6.0;
        let bg_w = text_w + 2.0 * pad_x;
        let bg_h = face_size + 2.0 * pad_y;
        let bg_x = button.cx + button.radius + 8.0;
        let bg_y = button.cy - bg_h / 2.0;

        fill_rect_rgba(
            &mut self.pixmap,
            bg_x, bg_y, bg_w, bg_h,
            20, 20, 30, 230,
        );
        outline_rect_rgba(
            &mut self.pixmap,
            bg_x, bg_y, bg_w, bg_h,
            (200, 200, 255), 2.0,
        );
        self.text.draw_text(
            &mut self.pixmap,
            label,
            bg_x + pad_x,
            bg_y + pad_y + face_size,
            face_size,
            SkColor::from_rgba8(255, 255, 255, 255),
        );
    }
}

// ============================================================
// Свободные функции: кнопки и иконки
// ============================================================

pub(crate) fn draw_button_label(
    pixmap: &mut Pixmap,
    text: &mut crate::text::TextRenderer,
    r: SetupRect,
    label: &str,
    bg: (u8, u8, u8),
) {
    fill_rect_rgba(pixmap, r.x, r.y, r.w, r.h, bg.0, bg.1, bg.2, 255);
    outline_rect_rgba(pixmap, r.x, r.y, r.w, r.h, (255, 255, 255), 2.0);

    let face_size = 18.0;
    let tw = text.text_width(label, face_size);
    text.draw_text(
        pixmap,
        label,
        r.x + (r.w - tw) / 2.0,
                   r.y + r.h / 2.0 + 6.0,
                   face_size,
                   SkColor::from_rgba8(255, 255, 255, 255),
    );
}

pub(crate) fn color_label(c: Color) -> &'static str {
    match c {
        Color::White => "Белые",
        Color::Black => "Чёрные",
    }
}

fn draw_circle_rgba(
    pixmap: &mut Pixmap,
    cx: f32, cy: f32, r: f32,
    color: (u8, u8, u8), a: u8,
) {
    let mut paint = Paint::default();
    paint.set_color(SkColor::from_rgba8(color.0, color.1, color.2, a));
    let mut pb = PathBuilder::new();
    pb.push_circle(cx, cy, r);
    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
                         None,
        );
    }
}

fn draw_circle_outline_rgba(
    pixmap: &mut Pixmap,
    cx: f32, cy: f32, r: f32,
    color: (u8, u8, u8), width: f32,
) {
    let mut paint = Paint::default();
    paint.set_color(SkColor::from_rgba8(color.0, color.1, color.2, 255));
    let mut stroke = Stroke::default();
    stroke.width = width;
    let mut pb = PathBuilder::new();
    pb.push_circle(cx, cy, r);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

fn brighten(c: (u8, u8, u8), delta: u8) -> (u8, u8, u8) {
    (
        c.0.saturating_add(delta),
     c.1.saturating_add(delta),
     c.2.saturating_add(delta),
    )
}

fn draw_flip_icon(pixmap: &mut Pixmap, cx: f32, cy: f32, size: f32, color: (u8, u8, u8)) {
    let s = size;
    let r = s * 0.32;
    let mut paint = Paint::default();
    paint.set_color(SkColor::from_rgba8(color.0, color.1, color.2, 255));
    let mut stroke = Stroke::default();
    stroke.width = 3.0;
    stroke.line_cap = tiny_skia::LineCap::Round;

    let mut pb = PathBuilder::new();
    pb.move_to(cx - r, cy - r * 0.5);
    pb.quad_to(cx, cy - r * 1.6, cx + r, cy - r * 0.5);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    let mut pb = PathBuilder::new();
    pb.move_to(cx + r, cy + r * 0.5);
    pb.quad_to(cx, cy + r * 1.6, cx - r, cy + r * 0.5);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    let arrow = s * 0.12;
    let mut pb = PathBuilder::new();
    pb.move_to(cx + r - arrow, cy - r * 0.5 - arrow);
    pb.line_to(cx + r, cy - r * 0.5);
    pb.line_to(cx + r - arrow, cy - r * 0.5 + arrow);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    let mut pb = PathBuilder::new();
    pb.move_to(cx - r + arrow, cy + r * 0.5 + arrow);
    pb.line_to(cx - r, cy + r * 0.5);
    pb.line_to(cx - r + arrow, cy + r * 0.5 - arrow);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

fn draw_flag_icon(pixmap: &mut Pixmap, cx: f32, cy: f32, size: f32, color: (u8, u8, u8)) {
    let s = size;
    let pole_x = cx - s * 0.28;
    let pole_top = cy - s * 0.32;
    let pole_bottom = cy + s * 0.32;

    let mut paint = Paint::default();
    paint.set_color(SkColor::from_rgba8(color.0, color.1, color.2, 255));
    let mut stroke = Stroke::default();
    stroke.width = 4.0;
    stroke.line_cap = tiny_skia::LineCap::Round;

    let mut pb = PathBuilder::new();
    pb.move_to(pole_x, pole_top);
    pb.line_to(pole_x, pole_bottom);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    let mut pb = PathBuilder::new();
    pb.move_to(pole_x - s * 0.10, pole_bottom);
    pb.line_to(pole_x + s * 0.10, pole_bottom);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    let mut pb = PathBuilder::new();
    pb.move_to(pole_x, pole_top);
    pb.line_to(pole_x + s * 0.45, pole_top + s * 0.16);
    pb.line_to(pole_x, pole_top + s * 0.32);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
                         None,
        );
    }
}

fn draw_handshake_icon(pixmap: &mut Pixmap, cx: f32, cy: f32, size: f32, color: (u8, u8, u8)) {
    let s = size;
    let mut paint = Paint::default();
    paint.set_color(SkColor::from_rgba8(color.0, color.1, color.2, 255));
    let mut stroke = Stroke::default();
    stroke.width = 3.0;
    stroke.line_cap = tiny_skia::LineCap::Round;

    let mut pb = PathBuilder::new();
    pb.move_to(cx - s * 0.35, cy - s * 0.10);
    pb.line_to(cx - s * 0.05, cy + s * 0.05);
    pb.line_to(cx + s * 0.05, cy + s * 0.05);
    pb.line_to(cx + s * 0.35, cy - s * 0.10);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    for i in 0..3 {
        let fx = cx - s * 0.20 + i as f32 * s * 0.10;
        let mut pb = PathBuilder::new();
        pb.move_to(fx, cy + s * 0.05);
        pb.line_to(fx + s * 0.04, cy + s * 0.18);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
}
