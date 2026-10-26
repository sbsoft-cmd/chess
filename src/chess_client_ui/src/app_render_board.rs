//! Отрисовка доски, фигур, подсветок, запаса, часов, имён,
//! game-over блока, а также экрана настройки.
//!
//! Кнопки, tooltip и иконки — в `app_render_buttons.rs`.
//! Оверлеи (превращение, ничья, отключение, ожидание, конец игры) —
//! в `app_render_overlays.rs`.

pub(crate) const BOARD_COLS_U: u32 = 16;
pub(crate) const BOARD_ROWS_U: u32 = 10;

use chess_logic::{GameMode, Seat};
use crate::app::{
    App, BgKey, Phase, BOARD_BORDER, BG,
    CLOCK_RED_THRESHOLD_SECS, DARK, HL_CHECK, HL_MOVE_DOT, HL_SELECTED,
    LIGHT, NAME_NORMAL,
    SETUP_BG, SETUP_FIELD_BG, SETUP_FIELD_BORDER, SETUP_FIELD_BORDER_FOCUS,
    SETUP_OPTION_BG, SETUP_OPTION_BG_SELECTED, SETUP_OPTION_BORDER,
    SETUP_OPTION_BORDER_SELECTED, SETUP_OPTION_DISABLED, SETUP_PLAY_ACTIVE,
    SETUP_PLAY_INACTIVE, SETUP_TEXT, SETUP_TEXT_DIM, SETUP_TITLE,
};
use crate::app::render_buttons::color_label;
use crate::layout::{LEFT_PANEL, MARGIN, TOP_MARGIN};
use tracing::info;

use crate::setup::{
    Rect as SetupRect, TIME_OPTIONS, VARIANT_10X10, VARIANT_10X10_FREESTYLE, VARIANT_8X8,
};
use anyhow::{anyhow, Result};
use chess_logic::{Board, Color, EndReason, PieceType, Square};
use std::time::Duration;
use tiny_skia::{
    Color as SkColor, Paint, PathBuilder, Pixmap, PixmapPaint, Rect as SkRect, Stroke,
    Transform,
};

impl App {
    pub(super) fn render(&mut self) -> Result<()> {
        match self.phase {
            Phase::Setup => self.render_setup(),
            Phase::Playing => self.render_game(),
        }

        let surface = match self.surface.as_mut() {
            Some(s) => s,
            None => return Ok(()),
        };

        let count = (self.width as usize) * (self.height as usize);

        let mut buffer = surface
        .buffer_mut()
        .map_err(|e| anyhow!("surface.buffer_mut: {e}"))?;

        let src = self.pixmap.data();
        let pixels = buffer.as_mut();

        for (i, chunk) in src.chunks_exact(4).enumerate() {
            if i >= count {
                break;
            }
            let r = chunk[0] as u32;
            let g = chunk[1] as u32;
            let b = chunk[2] as u32;
            pixels[i] = (r << 16) | (g << 8) | b;
        }

        buffer
        .present()
        .map_err(|e| anyhow!("buffer.present: {e}"))?;

        Ok(())
    }

    // ============================================================
    // SETUP
    // ============================================================

    pub(super) fn render_setup(&mut self) {
        let w = self.width as f32;
        self.pixmap
        .fill(SkColor::from_rgba8(SETUP_BG.0, SETUP_BG.1, SETUP_BG.2, 255));

        let text_color = SkColor::from_rgba8(SETUP_TEXT.0, SETUP_TEXT.1, SETUP_TEXT.2, 255);
        let text_dim =
        SkColor::from_rgba8(SETUP_TEXT_DIM.0, SETUP_TEXT_DIM.1, SETUP_TEXT_DIM.2, 255);
        let title_color =
        SkColor::from_rgba8(SETUP_TITLE.0, SETUP_TITLE.1, SETUP_TITLE.2, 255);

        let cx = w / 2.0;

        let title = "Настройки";
        let title_size = 32.0;
        let tw = self.text.text_width(title, title_size);
        self.text.draw_text(
            &mut self.pixmap,
            title,
            cx - tw / 2.0,
            80.0,
            title_size,
            title_color,
        );

        let label_size = 18.0;

        // --- 3 поля имён ---
        // partner_warning: выбран 16×10, но поле «союзник» пустое → подсветить красным.
        let partner_warning = self.setup.variant == crate::setup::VARIANT_16X10
        && self.setup.partner_name.trim().is_empty();

        let draw_field = |pixmap: &mut Pixmap,
        text: &mut crate::text::TextRenderer,
        rect: crate::setup::Rect,
        value: &str,
        placeholder: &str,
        focused: bool,
        warning: bool,
        text_color: tiny_skia::Color,
        text_dim: tiny_skia::Color,
        warn_color: tiny_skia::Color| {
            let border = if warning {
                (255, 120, 120)
            } else if focused {
                SETUP_FIELD_BORDER_FOCUS
            } else {
                SETUP_FIELD_BORDER
            };
            fill_rect(pixmap, rect, SETUP_FIELD_BG);
            outline_rect(pixmap, rect, border, 2.0);
            let (display, color) = if value.is_empty() {
                (placeholder, if warning { warn_color } else { text_dim })
            } else {
                (value, text_color)
            };
            text.draw_text(
                pixmap,
                display,
                rect.x + 12.0,
                rect.y + rect.h / 2.0 + 6.0,
                label_size,
                color,
            );
        };

        let warn_color = SkColor::from_rgba8(255, 160, 160, 255);

        draw_field(
            &mut self.pixmap,
            &mut self.text,
            self.setup_layout.name_field,
            &self.setup.name,
            "(введите своё имя)",
                   self.setup.focused_field == crate::setup::FocusField::Name,
                   false,
                   text_color,
                   text_dim,
                   warn_color,
        );
        draw_field(
            &mut self.pixmap,
            &mut self.text,
            self.setup_layout.opponent_field,
            &self.setup.opponent_name,
            if self.setup.variant == crate::setup::VARIANT_16X10 {
                "(соперники через , )"
            } else {
                "(соперник)"
            },
            self.setup.focused_field == crate::setup::FocusField::Opponent,
            false,
            text_color,
            text_dim,
            warn_color,
        );
        draw_field(
            &mut self.pixmap,
            &mut self.text,
            self.setup_layout.partner_field,
            &self.setup.partner_name,
            "(союзник)",
                   self.setup.focused_field == crate::setup::FocusField::Partner,
                   false,
                   text_color,
                   text_dim,
                   warn_color, // параметр остаётся, но не используется
        );

        // --- 5 вариантов ---
        draw_option(
            &mut self.pixmap, &mut self.text, self.setup_layout.variant_8,
            "8×8", self.setup.variant == crate::setup::VARIANT_8X8, true, 16.0,
        );
        draw_option(
            &mut self.pixmap, &mut self.text, self.setup_layout.variant_8_free,
            "8×8 free", self.setup.variant == crate::setup::VARIANT_8X8_FREESTYLE, true, 16.0,
        );
        draw_option(
            &mut self.pixmap, &mut self.text, self.setup_layout.variant_10,
            "10×10", self.setup.variant == crate::setup::VARIANT_10X10, true, 16.0,
        );
        draw_option(
            &mut self.pixmap, &mut self.text, self.setup_layout.variant_10_free,
            "10×10 free", self.setup.variant == crate::setup::VARIANT_10X10_FREESTYLE, true, 16.0,
        );
        draw_option(
            &mut self.pixmap, &mut self.text, self.setup_layout.variant_16,
            "16×10", self.setup.variant == crate::setup::VARIANT_16X10, true, 16.0,
        );

        // --- 11 кнопок времени ---
        for (i, opt) in TIME_OPTIONS.iter().enumerate() {
            if i >= self.setup_layout.time_rects.len() {
                break;
            }
            let r = self.setup_layout.time_rects[i];
            let selected = self.setup.time_control == opt.value;
            draw_option(
                &mut self.pixmap,
                &mut self.text,
                r,
                opt.label,
                selected,
                true,
                16.0,
            );
        }

        // --- 2 кнопки роли (debug) ---
        #[cfg(debug_assertions)]
        {
            draw_option(
                &mut self.pixmap,
                &mut self.text,
                self.setup_layout.role_human,
                "Человек",
                self.setup.role == crate::setup::Role::Human,
                true,
                16.0,
            );
            draw_option(
                &mut self.pixmap,
                &mut self.text,
                self.setup_layout.role_bot,
                "Бот",
                self.setup.role == crate::setup::Role::Bot,
                true,
                16.0,
            );
        }

        // --- Играть ---
        let can_start = self.setup.can_start();
        let pb = self.setup_layout.play_button;
        let play_bg = if can_start {
            SETUP_PLAY_ACTIVE
        } else {
            SETUP_PLAY_INACTIVE
        };
        fill_rect(&mut self.pixmap, pb, play_bg);
        outline_rect(
            &mut self.pixmap,
            pb,
            if can_start {
                (120, 220, 120)
            } else {
                (100, 100, 110)
            },
            3.0,
        );
        let play_label = "Играть";
        let pl_size = 22.0;
        let pl_w = self.text.text_width(play_label, pl_size);
        let play_text_color = if can_start { text_color } else { text_dim };
        self.text.draw_text(
            &mut self.pixmap,
            play_label,
            pb.x + (pb.w - pl_w) / 2.0,
                            pb.y + pb.h / 2.0 + 8.0,
                            pl_size,
                            play_text_color,
        );

        self.draw_setup_overlay();
    }
    // ============================================================
    // GAME
    // ============================================================

    pub(super) fn render_game(&mut self) {
        if self.waiting_for_opponent {
            self.pixmap.fill(SkColor::from_rgba8(20, 20, 30, 255));
            self.draw_waiting_overlay();
            return;
        }

        let (board_hash, current_player, is_over) = match self.state.as_ref() {
            Some(st) => (
                board_fingerprint(&st.board, self.layout.cols(), self.layout.rows()),
                         st.current_player,
                         st.is_over(),
            ),
            None => (0u64, Color::White, false),
        };

        let key = BgKey {
            board_hash,
            selected: self.selected.map(|s| (s.col, s.row)),
            current_player,
            is_over,
            my_color: self.my_color,
            board_flipped: self.board_flipped,
            variant_tag: 0,
            dragging: self.dragging,
            drag_from: self.drag_from.map(|s| (s.col, s.row)),
            dragging_reserve: self.dragging_reserve,
            drag_from_reserve: self.drag_from_reserve,
            draw_offer_from: self.draw_offer_from,
            hovered_button: self.hovered_button,
            promotion_open: self.promotion_pending.is_some(),
            disconnect_open: self.disconnect_overlay.is_some(),
            waiting: self.waiting_for_opponent,
            last_move_from: self.last_move_from.map(|s| (s.col, s.row)),
            last_move_to: self.last_move_to.map(|s| (s.col, s.row)),
        };

        if self.bg_key.as_ref() != Some(&key) {
            self.redraw_bg(&key);
        }

        {
            let bg_data = self.bg_pixmap.data();
            let dst = self.pixmap.data_mut();
            dst.copy_from_slice(bg_data);
        }

        self.draw_clocks();

        // Drag фигуры с доски.
        if self.dragging {
            if let Some(from) = self.drag_from {
                if let Some(st) = self.state.as_ref() {
                    if let Some(piece) = st.board.get(from) {
                        if let Some(img) = pick_atlas(&self.atlas, piece.color, piece.piece_type, false) {
                            let img_paint = PixmapPaint { opacity: 1.0, blend_mode: tiny_skia::BlendMode::SourceOver, quality: tiny_skia::FilterQuality::Nearest };
                            let iw = img.width() as f32;
                            let ih = img.height() as f32;
                            self.pixmap.draw_pixmap((self.drag_pixel_x - iw / 2.0) as i32, (self.drag_pixel_y - ih / 2.0) as i32, img.as_ref(), &img_paint, Transform::identity(), None);
                        }
                    }
                }
            }
        }

        // Drag фигуры из запаса (1х1).
        if self.dragging_reserve {
            if let Some((color, pt)) = self.drag_from_reserve {
                if let Some(img) = pick_atlas(&self.atlas, color, pt, false) {
                    let img_paint = PixmapPaint { opacity: 1.0, blend_mode: tiny_skia::BlendMode::SourceOver, quality: tiny_skia::FilterQuality::Nearest };
                    let iw = img.width() as f32;
                    let ih = img.height() as f32;
                    self.pixmap.draw_pixmap((self.drag_pixel_x - iw / 2.0) as i32, (self.drag_pixel_y - ih / 2.0) as i32, img.as_ref(), &img_paint, Transform::identity(), None);
                }
            }
        }

        // Drag Royal'а из укрытия (2х2).
        if let Some((pt, _seat)) = self.dragging_hidden_royal {
            let color = self.my_color.unwrap_or(Color::White);
            if let Some(img) = pick_atlas(&self.atlas, color, pt, true) {
                let img_paint = PixmapPaint { opacity: 1.0, blend_mode: tiny_skia::BlendMode::SourceOver, quality: tiny_skia::FilterQuality::Nearest };
                let iw = img.width() as f32;
                let ih = img.height() as f32;
                self.pixmap.draw_pixmap((self.drag_pixel_x - iw / 2.0) as i32, (self.drag_pixel_y - ih / 2.0) as i32, img.as_ref(), &img_paint, Transform::identity(), None);
            }
        }

        if self.promotion_pending.is_some() { self.draw_promotion_overlay(); }
        if self.draw_offer_from.is_some() { self.draw_draw_offer_overlay(); }
        if self.disconnect_overlay.is_some() { self.draw_disconnect_overlay(); }
        if self.draw_request_from_partner.is_some() { self.draw_draw_request_banner(); }
        if self.draw_offer_2v2.is_some() { self.draw_draw_offer_2v2_overlay(); }

        // Баннер сдачи.
        if self.resign_request_from_partner.is_some() { self.draw_resign_request_banner(); }

        let show_settings = self.state.as_ref().map_or(false, |st| st.is_over()) || self.disconnect_overlay.is_some();
        if show_settings { self.draw_game_over_button(); }

        if self.transient_message.is_some() {
            let msg = self.transient_message.as_ref().map(|(m, _)| m.clone()).unwrap_or_default();
            self.draw_transient_message(&msg);
        }


        if let Some(st) = self.state.as_ref() {
            if st.is_over() {
                info!(
                    "[ui] render_game: is_over=true, history={}, review_index={:?}, phase={:?}",
                    self.history.len(), self.review_index, self.phase
                );
            }
        }
        // --- Кнопки истории (если игра окончена) ---
        if self.state.as_ref().map_or(false, |st| st.is_over()) && !self.history.is_empty() {
            for (action, rect) in self.review_buttons() {
                let active = self.review_index.is_some();
                let bg = if active { (60, 100, 160) } else { (40, 40, 55) };
                fill_rect_rgba(&mut self.pixmap, rect.x, rect.y, rect.w, rect.h, bg.0, bg.1, bg.2, 255);
                outline_rect_rgba(&mut self.pixmap, rect.x, rect.y, rect.w, rect.h, (150, 150, 180), 2.0);

                let label = match action {
                    crate::app::ReviewAction::Start => "|<",
                    crate::app::ReviewAction::Prev => "<",
                    crate::app::ReviewAction::Next => ">",
                    crate::app::ReviewAction::End => ">|",
                };
                let tw = self.text.text_width(label, 16.0);
                self.text.draw_text(
                    &mut self.pixmap,
                    label,
                    rect.x + (rect.w - tw) / 2.0,
                                    rect.y + rect.h / 2.0 + 6.0,
                                    16.0,
                                    SkColor::from_rgba8(255, 255, 255, 255),
                );
            }
        }

        self.draw_tooltip_if_needed();
    }

    pub(super) fn redraw_bg(&mut self, key: &BgKey) {
        // Берем доску из истории, если просматриваем, иначе текущую.
        let board = if let Some(idx) = self.review_index {
            self.history.get(idx).map_or_else(
                || self.state.as_ref().map_or_else(Board::initial, |s| s.board.clone()),
                                              |snap| snap.board.clone(),
            )
        } else {
            self.state.as_ref().map_or_else(Board::initial, |s| s.board.clone())
        };
        let is_2v2 = self.state.as_ref().map_or(false, |s| s.mode == chess_logic::GameMode::Team2v2);

        self.bg_pixmap.fill(SkColor::from_rgba8(BG.0, BG.1, BG.2, 255));

        // Правая панель (запасные поля / Royal'ы в укрытии).
        let panel_x = self.layout.right() + MARGIN as f32;
        let panel_w = crate::layout::RIGHT_PANEL as f32;
        let panel_y = self.layout.top();
        let panel_h = self.layout.board_h as f32;
        fill_rect_rgba(
            &mut self.bg_pixmap,
            panel_x, panel_y, panel_w, panel_h,
            25, 25, 35, 255,
        );

        // Клетки доски.
        let mut paint = Paint::default();
        paint.anti_alias = false;
        for col in 0..self.layout.cols() {
            for row in 0..self.layout.rows() {
                let (r, g, b) = if (col + row) % 2 == 0 { LIGHT } else { DARK };
                paint.set_color(SkColor::from_rgba8(r, g, b, 255));
                let (x, y) = self.cell_origin(col, row);
                let cell = self.layout.cell as f32;
                if let Some(rect) = SkRect::from_xywh(x, y, cell, cell) {
                    self.bg_pixmap
                    .fill_rect(rect, &paint, Transform::identity(), None);
                }
            }
        }

        self.draw_highlights_bg();
        self.draw_check_highlight_bg();
        self.draw_reserve_bg();

        // Фигуры.
        let pixmap_paint = PixmapPaint {
            opacity: 1.0,
            blend_mode: tiny_skia::BlendMode::SourceOver,
            quality: tiny_skia::FilterQuality::Nearest,
        };

        // «Виртуальный сдвиг» на время ожидания State после хода:
        // рисуем фигуру с from на to, чтобы не было мигания.
        let pending = self.pending_move_draw;

        for col in 0..self.layout.cols() {
            for row in 0..self.layout.rows() {
                let sq = Square::new(col as i8, row as i8);

                // Фигура, которую сейчас тащат мышкой.
                if self.dragging && Some(sq) == self.drag_from {
                    continue;
                }

                // С учётом виртуального сдвига: если sq == from — клетка пуста,
                // если sq == to — берём фигуру с from, иначе — из board.
                let piece_opt: Option<chess_logic::Piece> = if let Some((from, to)) = pending {
                    if sq == from {
                        None
                    } else if sq == to {
                        board.get(from)
                    } else {
                        board.get(sq)
                    }
                } else {
                    board.get(sq)
                };

                let piece = match piece_opt {
                    Some(p) => p,
                    None => continue,
                };

                // Для определения "своей" фигуры используем исходную клетку
                // (owner принадлежит фигуре, а не клетке, куда её виртуально сдвинули).
                let owner_sq = if let Some((from, to)) = pending {
                    if sq == to { from } else { sq }
                } else {
                    sq
                };

                let is_mine = match (self.my_seat, self.my_color) {
                    (Some(seat), Some(my_color)) => {
                        board.get_owner(owner_sq) == Some(seat) && piece.color == my_color
                    }
                    _ => false,
                };

                // Используем обычную картинку (без зелёного контура).
                let img = pick_atlas(&self.atlas, piece.color, piece.piece_type, false);
                let img = match img {
                    Some(p) => p,
                    None => continue,
                };

                let (x, y) = self.cell_origin(col, row);
                let cell = self.layout.cell as f32;
                let iw = img.width() as f32;
                let ih = img.height() as f32;

                // Рисуем фигуру.
                self.bg_pixmap.draw_pixmap(
                    (x + (cell - iw) / 2.0) as i32,
                                           (y + (cell - ih) / 2.0) as i32,
                                           img.as_ref(),
                                           &pixmap_paint,
                                           Transform::identity(),
                                           None,
                );

                // Если это моя фигура — рисуем зелёные уголки.
                if is_mine {
                    draw_cell_corners(&mut self.bg_pixmap, x, y, cell, (0, 255, 0), 3.0);
                }
            }
        }

        // Рамка доски.
        let mut bp = Paint::default();
        bp.set_color(SkColor::from_rgba8(
            BOARD_BORDER.0,
            BOARD_BORDER.1,
            BOARD_BORDER.2,
            255,
        ));
        let mut st = Stroke::default();
        st.width = 2.0;
        let mut pb = PathBuilder::new();
        pb.move_to(self.layout.left(), self.layout.top());
        pb.line_to(self.layout.right(), self.layout.top());
        pb.line_to(self.layout.right(), self.layout.bottom());
        pb.line_to(self.layout.left(), self.layout.bottom());
        pb.close();
        if let Some(path) = pb.finish() {
            self.bg_pixmap
            .stroke_path(&path, &bp, &st, Transform::identity(), None);
        }

        self.draw_buttons_bg();
        self.draw_names_bg();
        self.draw_game_over_bg();

        self.bg_key = Some(key.clone());

        let _ = is_2v2;
    }


    pub(super) fn draw_highlights_bg(&mut self) {
        let cell = self.layout.cell as f32;
        let center_offset = cell / 2.0;

        // ============================================================
        // 1. Подсветка последнего хода (штриховка).
        //    Рисуется в любом режиме — и в игре, и в просмотре истории.
        // ============================================================
        // 1. Подсветка последнего хода (штриховка + чёрный контур).
        for sq in &self.last_move_squares {
            let (x, y) = self.cell_origin(sq.col as u32, sq.row as u32);
            //draw_last_move_highlight(&mut self.bg_pixmap, x, y, cell);
            // Чёрный контур вокруг клетки последнего хода —
            // аналогично красному контуру шаха, но чёрный.
            outline_cell(&mut self.bg_pixmap, x, y, cell, (0, 0, 0), 6.0);
        }

        // ============================================================
        // 2. Выделенная клетка (зелёная рамка).
        // ============================================================
        if let Some(sq) = self.selected {
            let (x, y) = self.cell_origin(sq.col as u32, sq.row as u32);
            outline_cell(&mut self.bg_pixmap, x, y, cell, HL_SELECTED, 4.0);
        }

        // ============================================================
        // 3. Обычные ходы (зелёные точки).
        // ============================================================
        for m in &self.legal_moves {
            if self.legal_castles.contains(m) {
                continue;
            }
            let (x, y) = self.cell_origin(m.col as u32, m.row as u32);
            draw_circle_filled(
                &mut self.bg_pixmap,
                x + center_offset,
                y + center_offset,
                8.0,
                HL_MOVE_DOT,
            );
        }

        // ============================================================
        // 4. Ходы из запаса (зелёные точки) — для 1х1.
        // ============================================================
        for m in &self.drop_moves {
            let (x, y) = self.cell_origin(m.col as u32, m.row as u32);
            draw_circle_filled(
                &mut self.bg_pixmap,
                x + center_offset,
                y + center_offset,
                8.0,
                HL_MOVE_DOT,
            );
        }

        // ============================================================
        // 5. Рокировки (зелёные кольца).
        // ============================================================
        for m in &self.legal_castles {
            let (x, y) = self.cell_origin(m.col as u32, m.row as u32);
            draw_circle_outline_rgba(
                &mut self.bg_pixmap,
                x + center_offset,
                y + center_offset,
                cell * 0.35,
                (0, 255, 0),
                                     4.0,
            );
        }

        // ============================================================
        // 6. Подсветка клеток для возврата Royal'а (2×2).
        // ============================================================
        if !self.my_hidden_royals.is_empty() {
            if let (Some(st), Some(my_color)) = (self.state.as_ref(), self.my_color) {
                let squares = st.legal_royal_return_squares(my_color);
                for sq in squares {
                    let (x, y) = self.cell_origin(sq.col as u32, sq.row as u32);
                    let cell = self.layout.cell as f32;
                    fill_rect_rgba(&mut self.bg_pixmap, x, y, cell, cell, 0, 255, 0, 60);
                    outline_cell(&mut self.bg_pixmap, x, y, cell, (0, 255, 0), 3.0);
                }
            }
        }
    }

    pub(super) fn draw_check_highlight_bg(&mut self) {
        // Берём доску и режим из того же источника, что и redraw_bg:
        // в режиме просмотра истории — из снимка, иначе — из текущего state.
        let (board, is_2v2) = if let Some(idx) = self.review_index {
            let snap = match self.history.get(idx) {
                Some(s) => s,
                None => return,
            };
            // Режим определяем по текущему state (mode не меняется по ходу партии).
            let is_2v2 = self
            .state
            .as_ref()
            .map_or(false, |s| s.mode == chess_logic::GameMode::Team2v2);
            (&snap.board, is_2v2)
        } else {
            let st = match self.state.as_ref() {
                Some(s) => s,
                None => return,
            };
            (&st.board, st.mode == chess_logic::GameMode::Team2v2)
        };

        // Проверяем обоих: и белых, и чёрных. Шах может быть объявлен стороне,
        // которая еще не ходит (если это первый ход из двух).
        let mut targets: Vec<Square> = Vec::new();

        for col in 0..self.layout.cols() {
            for row in 0..self.layout.rows() {
                let sq = Square::new(col as i8, row as i8);
                if let Some(p) = board.get(sq) {
                    let is_royal = if is_2v2 {
                        p.piece_type.is_royal_2v2()
                    } else {
                        p.piece_type == PieceType::King
                    };

                    if is_royal
                        && chess_logic::is_square_attacked(
                            board,
                            sq.col,
                            sq.row,
                            p.color.opposite(),
                        )
                        {
                            targets.push(sq);
                        }
                }
            }
        }

        for k in targets {
            let (x, y) = self.cell_origin(k.col as u32, k.row as u32);
            let cell = self.layout.cell as f32;
            fill_rect_rgba(&mut self.bg_pixmap, x, y, cell, cell, 220, 30, 30, 90);
            outline_cell(&mut self.bg_pixmap, x, y, cell, HL_CHECK, 5.0);
        }
    }

    pub(super) fn draw_reserve_bg(&mut self) {
        let st = match self.state.as_ref() {
            Some(s) => s.clone(),
            None => return,
        };

        if st.mode == chess_logic::GameMode::Classic8x8 {
            return;
        }
        // ============================================================
        // 2х2: Royal'ы в укрытии
        // ============================================================
        if st.mode == chess_logic::GameMode::Team2v2 {
            // Если мы в режиме просмотра истории, берём данные из снимка.
            let (white_team, black_team) = if let Some(idx) = self.review_index {
                if let Some(snap) = self.history.get(idx) {
                    (snap.white_team.clone(), snap.black_team.clone())
                } else {
                    (st.white_team.clone(), st.black_team.clone())
                }
            } else {
                (st.white_team.clone(), st.black_team.clone())
            };

            let panel_x = self.layout.right() + MARGIN as f32;
            let slot_size = self.layout.cell as f32 * 0.8;
            let gap = 10.0;
            let mut cy = self.layout.top() + slot_size / 2.0;

            for color in [Color::White, Color::Black] {
                let team = match color {
                    Color::White => &white_team,
                    Color::Black => &black_team,
                };

                for (seat_idx, seat) in [Seat::First, Seat::Second].iter().enumerate() {
                    let p = &team.players[seat_idx];

                    for pt in [PieceType::Prince, PieceType::Princess] {
                        let in_holding = p.is_in_holding(pt);
                        let cd = p.get_cd(pt);

                        if in_holding {
                            let x = panel_x + slot_size / 2.0;

                            // Рисуем слот.
                            fill_rect_rgba(
                                &mut self.bg_pixmap,
                                x - slot_size / 2.0, cy - slot_size / 2.0,
                                slot_size, slot_size,
                                40, 40, 55, 255,
                            );

                            // Зелёный контур, если можно вернуть (cd == 0).
                            let border = if cd == 0 {
                                (0, 255, 0) // Зелёный
                            } else {
                                (100, 100, 130) // Серый
                            };
                            outline_rect_rgba(
                                &mut self.bg_pixmap,
                                x - slot_size / 2.0, cy - slot_size / 2.0,
                                slot_size, slot_size,
                                border, 3.0,
                            );

                            // Рисуем фигуру Royal'а (если её не тащат).
                            let is_dragged = self.dragging_hidden_royal == Some((pt, *seat));
                            if !is_dragged {
                                if let Some(img) = pick_atlas(&self.atlas, color, pt, true) {
                                    let iw = img.width() as f32;
                                    let ih = img.height() as f32;
                                    let target = slot_size * 0.7;
                                    let scale = (target / iw).min(target / ih);
                                    let sw = iw * scale;
                                    let sh = ih * scale;
                                    let img_paint = PixmapPaint {
                                        opacity: if cd == 0 { 1.0 } else { 0.5 },
                                        blend_mode: tiny_skia::BlendMode::SourceOver,
                                        quality: tiny_skia::FilterQuality::Bicubic,
                                    };
                                    let mut t = Transform::identity();
                                    t = t.pre_scale(scale, scale);
                                    t = t.post_translate(x - sw / 2.0, cy - sh / 2.0);
                                    self.bg_pixmap.draw_pixmap(0, 0, img.as_ref(), &img_paint, t, None);
                                }
                            }

                            // Рисуем CD (если > 0).
                            if cd > 0 {
                                let label = format!("{}", cd);
                                let face_size = 16.0;
                                let tw = self.text.text_width(&label, face_size);
                                self.text.draw_text(
                                    &mut self.bg_pixmap,
                                    &label,
                                    x + slot_size / 2.0 - tw - 4.0,
                                    cy + slot_size / 2.0 - 4.0,
                                    face_size,
                                    SkColor::from_rgba8(255, 100, 100, 255),
                                );
                            }

                            cy += slot_size + gap;
                        }
                    }
                }
            }
            return; // Для 2х2 больше ничего не рисуем.
        }

        // ============================================================
        // 1х1: обычный запас
        // ============================================================
        // Если мы в режиме просмотра истории, берём данные из снимка.
        let (reserve, reincarnating, current_player, is_over) = if let Some(idx) = self.review_index {
            if let Some(snap) = self.history.get(idx) {
                (
                    snap.reserve.clone(),
                 snap.reincarnating.clone(),
                 snap.current_player,
                 false, // В истории игра не считается оконченной (мы смотрим середину).
                )
            } else {
                (
                    st.reserve.clone(),
                 st.reincarnating.clone(),
                 st.current_player,
                 st.is_over(),
                )
            }
        } else {
            (
                st.reserve.clone(),
             st.reincarnating.clone(),
             st.current_player,
             st.is_over(),
            )
        };

        for slot in &self.reserve_slots {
            let half = slot.size / 2.0;
            let x = slot.cx - half;
            let y = slot.cy - half;

            let in_reserve = reserve
            .get(&slot.color)
            .map_or(false, |v| v.contains(&slot.piece_type));

            let bg = if in_reserve { (40, 40, 55) } else { (20, 20, 30) };
            fill_rect_rgba(
                &mut self.bg_pixmap,
                x, y, slot.size, slot.size,
                bg.0, bg.1, bg.2, 255,
            );

            let border = if self.selected_reserve == Some((slot.color, slot.piece_type)) {
                (80, 140, 255)
            } else {
                (100, 100, 130)
            };
            outline_rect_rgba(&mut self.bg_pixmap, x, y, slot.size, slot.size, border, 3.0);

            if in_reserve {
                let is_mine = self.my_color.map_or(false, |my_color| my_color == slot.color);

                let img_opt = if is_mine {
                    pick_atlas_mine(&self.atlas, slot.color, slot.piece_type, true)
                } else {
                    pick_atlas(&self.atlas, slot.color, slot.piece_type, true)
                };

                if let Some(img) = img_opt {
                    let dim = is_over
                    || (Some(slot.color) != self.my_color && current_player != slot.color);

                    let iw = img.width() as f32;
                    let ih = img.height() as f32;
                    let target = slot.size * 0.7;
                    let scale = (target / iw).min(target / ih);
                    let sw = iw * scale;
                    let sh = ih * scale;

                    let img_paint = PixmapPaint {
                        opacity: if dim { 0.35 } else { 1.0 },
                        blend_mode: tiny_skia::BlendMode::SourceOver,
                        quality: tiny_skia::FilterQuality::Bicubic,
                    };

                    let mut t = Transform::identity();
                    t = t.pre_scale(scale, scale);
                    t = t.post_translate(slot.cx - sw / 2.0, slot.cy - sh / 2.0);

                    self.bg_pixmap
                    .draw_pixmap(0, 0, img.as_ref(), &img_paint, t, None);
                }
            }

            if slot.piece_type == PieceType::Pawn {
                let q = reincarnating.get(&slot.color).map_or(0, |v| v.len());
                if q > 0 {
                    let label = format!("{}", q);
                    let face_size = 16.0;
                    let w = self.text.text_width(&label, face_size);
                    self.text.draw_text(
                        &mut self.bg_pixmap,
                        &label,
                        slot.cx + half - w - 6.0,
                        slot.cy - half + face_size + 2.0,
                        face_size,
                        SkColor::from_rgba8(255, 220, 100, 255),
                    );
                }
            }
        }
    }


    pub(super) fn draw_names_bg(&mut self) {
        let face_size = 14.0;
        let is_2v2 = self.state.as_ref().map_or(false, |s| s.mode == chess_logic::GameMode::Team2v2);
        let is_over = self.state.as_ref().map_or(false, |s| s.is_over());
        let current = self.state.as_ref().map(|s| s.current_player);

        let (top_color, bottom_color) = if self.board_flipped {
            (Color::White, Color::Black)
        } else {
            (Color::Black, Color::White)
        };

        // Формируем имена (два игрока или одно).
        let (top_names, bottom_names): (Vec<String>, Vec<String>) = if is_2v2 {
            let my_team = vec![
                self.my_name.clone(),
                self.partner_name.clone().unwrap_or_default(),
            ];
            let opp_team = if let Some(ref opp) = self.opponents {
                vec![opp[0].clone(), opp[1].clone()]
            } else {
                vec!["Соперник".to_string()]
            };

            if Some(top_color) == self.my_color {
                (my_team, opp_team)
            } else {
                (opp_team, my_team)
            }
        } else {
            // 1x1.
            let top = if Some(top_color) == self.my_color {
                vec![if self.my_name.is_empty() { "Я".to_string() } else { self.my_name.clone() }]
            } else {
                vec![self.opponent_name.clone().unwrap_or_else(|| "Соперник".to_string())]
            };
            let bottom = if Some(bottom_color) == self.my_color {
                vec![if self.my_name.is_empty() { "Я".to_string() } else { self.my_name.clone() }]
            } else {
                vec![self.opponent_name.clone().unwrap_or_else(|| "Соперник".to_string())]
            };
            (top, bottom)
        };

        // Рисуем верхнее имя.
        let top_highlighted = !is_over && current == Some(top_color);
        self.draw_player_names(
            &top_names,
            (LEFT_PANEL as f32) - MARGIN as f32,
                               (TOP_MARGIN as f32) + 32.0,
                               face_size,
                               top_highlighted,
                               true, // рисовать сверху вниз
        );

        // Рисуем нижнее имя.
        let board_bottom = (TOP_MARGIN as f32) + (self.layout.board_h as f32);
        let bottom_highlighted = !is_over && current == Some(bottom_color);
        self.draw_player_names(
            &bottom_names,
            (LEFT_PANEL as f32) - MARGIN as f32,
                               board_bottom - 8.0,
                               face_size,
                               bottom_highlighted,
                               false, // рисовать снизу вверх
        );
    }

    /// Рисует список имён. Если имена не влезают в одну строку, переносит на следующую.
    /// `draw_down` = true, если рисуем сверху вниз (для верхнего игрока);
    /// false, если снизу вверх (для нижнего).
    fn draw_player_names(
        &mut self,
        names: &[String],
        right_x: f32,
        baseline_y: f32,
        face_size: f32,
        highlighted: bool,
        draw_down: bool,
    ) {
        let max_width = (LEFT_PANEL as f32) - 2.0 * MARGIN as f32; // доступная ширина
        let line_h = face_size + 2.0;

        // Разбиваем имена на строки.
        let mut lines: Vec<String> = Vec::new();
        let mut current_line = String::new();
        for name in names {
            if name.is_empty() { continue; }
            let candidate = if current_line.is_empty() {
                name.clone()
            } else {
                format!("{}, {}", current_line, name)
            };

            if self.text.text_width(&candidate, face_size) <= max_width {
                current_line = candidate;
            } else {
                if !current_line.is_empty() {
                    lines.push(current_line.clone());
                }
                current_line = name.clone();
            }
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }

        // Рисуем строки.
        for (i, line) in lines.iter().enumerate() {
            let y = if draw_down {
                baseline_y + i as f32 * line_h
            } else {
                baseline_y - (lines.len() - 1 - i) as f32 * line_h
            };
            self.draw_player_name(line, right_x, y, face_size, highlighted);
        }
    }

    fn draw_player_name(
        &mut self,
        name: &str,
        right_x: f32,
        baseline_y: f32,
        face_size: f32,
        highlighted: bool,
    ) {
        let w = self.text.text_width(name, face_size);
        let x = right_x - w;

        if highlighted {
            let pad_x = 8.0;
            let pad_y = 4.0;
            fill_rect_rgba(
                &mut self.bg_pixmap,
                x - pad_x,
                baseline_y - face_size - pad_y,
                w + 2.0 * pad_x,
                face_size + 2.0 * pad_y,
                255, 255, 255, 255,
            );
            self.text.draw_text(
                &mut self.bg_pixmap,
                name,
                x,
                baseline_y,
                face_size,
                SkColor::from_rgba8(0, 0, 0, 255),
            );
        } else {
            self.text.draw_text(
                &mut self.bg_pixmap,
                name,
                x,
                baseline_y,
                face_size,
                SkColor::from_rgba8(NAME_NORMAL.0, NAME_NORMAL.1, NAME_NORMAL.2, 255),
            );
        }
    }

    pub(super) fn draw_game_over_bg(&mut self) {
        // Если мы в режиме просмотра истории — не рисуем сообщение о конце игры.
        if self.review_index.is_some() {
            return;
        }

        let st = match self.state.as_ref() {
            Some(s) => s,
            None => return,
        };

        let (lines, color): (Vec<String>, (u8, u8, u8)) = match st.end_reason {
            // --- 2x2: коронация ---
            Some(EndReason::Coronation) => {
                let winner = st.winner.unwrap_or(Color::White);
                (
                    vec![
                        "Коронация!".to_string(),
                 format!("Победа {}!", color_label(winner)),
                    ],
                 (255, 215, 0),
                )
            }

            // --- 2x2: сдача стороны ---
            Some(EndReason::Resign) if st.mode == chess_logic::GameMode::Team2v2 => {
                let winner = st.winner.unwrap_or(Color::White);
                let loser = winner.opposite();
                (
                    vec![
                        format!("{} сдались!", color_label(loser)),
                            format!("Победа {}!", color_label(winner)),
                    ],
                 (255, 120, 120),
                )
            }

            // --- duel: сдача ---
            _ if st.resigned_by.is_some() => {
                let loser = st.resigned_by.unwrap();
                let winner = loser.opposite();
                (
                    vec![
                        format!("{} сдались!", color_label(loser)),
                            format!("Победа {}!", color_label(winner)),
                    ],
                 (255, 120, 120),
                )
            }
            // --- duel: таймаут ---
            _ if st.winner.is_some()
            && matches!(st.end_reason, Some(EndReason::Timeout)) =>
            {
                let winner = st.winner.unwrap();
                let loser = winner.opposite();
                (
                    vec![
                        format!("{} просрочили время", color_label(loser)),
                            format!("Победа {}!", color_label(winner)),
                    ],
                 (255, 120, 120),
                )
            }
            // --- duel: мат ---
            _ if st.winner.is_some() => {
                let winner = st.winner.unwrap();
                (
                    vec![
                        "Мат!".to_string(),
                 format!("{} выиграли!", color_label(winner)),
                    ],
                 (255, 215, 0),
                )
            }
            // --- ничья ---
            _ if st.is_draw => {
                (vec!["Ничья".to_string()], (200, 200, 255))
            }
            _ => return,
        };

        // Отрисовка как раньше (без изменений):
        let cx = self.layout.left() + (self.layout.board_w as f32) / 2.0;
        let cy = self.layout.top() + (self.layout.board_h as f32) / 2.0;
        let face_size = 28.0;
        let line_h = face_size + 8.0;
        let total_h = line_h * lines.len() as f32;
        let max_w = lines
        .iter()
        .map(|l| self.text.text_width(l, face_size))
        .fold(0.0f32, f32::max);
        let pad = 24.0;
        let bg_w = max_w + 2.0 * pad;
        let bg_h = total_h + 2.0 * pad;
        let bg_x = cx - bg_w / 2.0;
        let bg_y = cy - bg_h / 2.0;

        fill_rect_rgba(&mut self.bg_pixmap, bg_x, bg_y, bg_w, bg_h, 0, 0, 0, 220);
        outline_rect_rgba(&mut self.bg_pixmap, bg_x, bg_y, bg_w, bg_h, color, 3.0);

        let mut y = bg_y + pad + face_size;
        for line in &lines {
            let w = self.text.text_width(line, face_size);
            self.text.draw_text(
                &mut self.bg_pixmap,
                line,
                cx - w / 2.0,
                y,
                face_size,
                SkColor::from_rgba8(color.0, color.1, color.2, 255),
            );
            y += line_h;
        }
    }

    // ============================================================
    // ЧАСЫ
    // ============================================================

    pub(super) fn draw_clocks(&mut self) {
        let Some((white, black)) = self.current_clock() else { return };
        let Some(st) = self.state.as_ref() else { return };

        let is_over = st.is_over();
        let current = st.current_player;

        let face_size = 22.0;

        let (top_color, bottom_color) = if self.board_flipped {
            (Color::White, Color::Black)
        } else {
            (Color::Black, Color::White)
        };

        let top_time = match top_color {
            Color::White => white,
            Color::Black => black,
        };
        let top_active = !is_over && current == top_color;
        let top_y = (TOP_MARGIN as f32) + 64.0;
        self.draw_clock(top_time, top_active, top_y, face_size);

        let bottom_time = match bottom_color {
            Color::White => white,
            Color::Black => black,
        };
        let board_bottom = (TOP_MARGIN as f32) + (self.layout.board_h as f32);
        let bottom_active = !is_over && current == bottom_color;
        let bottom_y = board_bottom - 40.0;
        self.draw_clock(bottom_time, bottom_active, bottom_y, face_size);
    }

    fn draw_clock(
        &mut self,
        time: Duration,
        active: bool,
        baseline_y: f32,
        face_size: f32,
    ) {
        let label = format_clock(time);
        let text_w = self.text.text_width(&label, face_size);

        let pad_x = 10.0;
        let pad_y = 4.0;
        let box_w = text_w + 2.0 * pad_x;
        let box_h = face_size + 2.0 * pad_y;

        let right_x = (LEFT_PANEL as f32) - MARGIN as f32;
        let box_x = right_x - box_w;
        let box_y = baseline_y - face_size - pad_y;

        let under_threshold = time.as_secs() < CLOCK_RED_THRESHOLD_SECS;
        let red_alert = under_threshold && active;

        let (bg_rgba, text_color) = if red_alert {
            ((180, 40, 40, 255), SkColor::from_rgba8(255, 255, 255, 255))
        } else if active {
            ((255, 255, 255, 255), SkColor::from_rgba8(0, 0, 0, 255))
        } else {
            ((40, 40, 55, 255), SkColor::from_rgba8(180, 180, 200, 255))
        };

        fill_rect_rgba(
            &mut self.pixmap,
            box_x, box_y, box_w, box_h,
            bg_rgba.0, bg_rgba.1, bg_rgba.2, bg_rgba.3,
        );

        if red_alert {
            outline_rect_rgba(
                &mut self.pixmap,
                box_x, box_y, box_w, box_h,
                (255, 100, 100), 2.0,
            );
        }

        self.text.draw_text(
            &mut self.pixmap,
            &label,
            box_x + pad_x,
            box_y + pad_y + face_size,
            face_size,
            text_color,
        );
    }
}

// ============================================================
// Свободные функции: доска, фигуры, хелперы отрисовки
// ============================================================

pub(crate) fn pick_atlas<'a>(
    atlas: &'a crate::assets::PieceAtlas,
    color: Color,
    pt: PieceType,
    in_reserve: bool,
) -> Option<&'a Pixmap> {
    let use_outline = match pt {
        PieceType::Eagle | PieceType::Cheetah | PieceType::Genie => true,
        PieceType::Pawn => in_reserve,
        _ => false,
    };
    if use_outline {
        atlas.get(color, pt)
    } else {
        atlas.get_plain(color, pt)
    }
}

/// Выбор Pixmap с жёлтым контуром — для своих фигур (owner == my_seat).
/// Логика обводки та же, что у `pick_atlas` (Eagle/Cheetah/Genie — всегда,
/// Pawn — только в слоте запаса, K/Q/R/B/N — без обводки), но слой —
/// `pixmaps_mine` вместо `pixmaps_plain`.
pub(crate) fn pick_atlas_mine<'a>(
    atlas: &'a crate::assets::PieceAtlas,
    color: Color,
    pt: PieceType,
    in_reserve: bool,
) -> Option<&'a Pixmap> {
    // Сначала пробуем жёлтую версию; если нет — fallback на plain.
    let use_outline = match pt {
        PieceType::Eagle | PieceType::Cheetah | PieceType::Genie => true,
        PieceType::Pawn => in_reserve,
        _ => false,
    };
    let _ = use_outline;
    atlas.get_mine(color, pt).or_else(|| atlas.get_plain(color, pt))
}


fn format_clock(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 60 {
        let m = secs / 60;
        let s = secs % 60;
        format!("{m}:{s:02}")
    } else {
        let s = secs;
        let ds = d.subsec_millis() / 100;
        format!("0:{s:02}.{ds}")
    }
}

pub(crate) fn fill_rect(pixmap: &mut Pixmap, r: SetupRect, color: (u8, u8, u8)) {
    let mut paint = Paint::default();
    paint.anti_alias = false;
    paint.set_color(SkColor::from_rgba8(color.0, color.1, color.2, 255));
    if let Some(rect) = SkRect::from_xywh(r.x, r.y, r.w, r.h) {
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

pub(crate) fn outline_rect(pixmap: &mut Pixmap, r: SetupRect, color: (u8, u8, u8), width: f32) {
    outline_rect_rgba(pixmap, r.x, r.y, r.w, r.h, color, width);
}

pub(crate) fn outline_rect_rgba(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: (u8, u8, u8),
                                width: f32,
) {
    let mut paint = Paint::default();
    paint.set_color(SkColor::from_rgba8(color.0, color.1, color.2, 255));
    let mut stroke = Stroke::default();
    stroke.width = width;
    let mut pb = PathBuilder::new();
    pb.move_to(x, y);
    pb.line_to(x + w, y);
    pb.line_to(x + w, y + h);
    pb.line_to(x, y + h);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

pub(crate) fn fill_rect_rgba(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) {
    let mut paint = Paint::default();
    paint.anti_alias = false;
    paint.blend_mode = tiny_skia::BlendMode::SourceOver;
    paint.set_color(SkColor::from_rgba8(r, g, b, a));
    if let Some(rect) = SkRect::from_xywh(x, y, w, h) {
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

fn outline_cell(pixmap: &mut Pixmap, x: f32, y: f32, size: f32, color: (u8, u8, u8), width: f32) {
    outline_rect_rgba(pixmap, x, y, size, size, color, width);
}

fn draw_circle_filled(pixmap: &mut Pixmap, cx: f32, cy: f32, r: f32, color: (u8, u8, u8)) {
    let mut paint = Paint::default();
    paint.set_color(SkColor::from_rgba8(color.0, color.1, color.2, 255));
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

fn draw_option(
    pixmap: &mut Pixmap,
    text: &mut crate::text::TextRenderer,
    r: SetupRect,
    label: &str,
    selected: bool,
    enabled: bool,
    font_size: f32,
) {
    let (bg, border, txt) = if !enabled {
        (SETUP_OPTION_BG, SETUP_OPTION_DISABLED, SETUP_TEXT_DIM)
    } else if selected {
        (
            SETUP_OPTION_BG_SELECTED,
         SETUP_OPTION_BORDER_SELECTED,
         SETUP_TEXT,
        )
    } else {
        (SETUP_OPTION_BG, SETUP_OPTION_BORDER, SETUP_TEXT)
    };

    fill_rect(pixmap, r, bg);
    outline_rect(pixmap, r, border, 2.0);

    let tw = text.text_width(label, font_size);
    let x = if r.w >= 220.0 {
        r.x + (r.w - tw) / 2.0
    } else {
        r.x + 16.0
    };
    text.draw_text(
        pixmap,
        label,
        x,
        r.y + r.h / 2.0 + 6.0,
        font_size,
        SkColor::from_rgba8(txt.0, txt.1, txt.2, 255),
    );
}

fn board_fingerprint(b: &Board, cols: u32, rows: u32) -> u64 {
    let mut items: Vec<(i8, i8, u8)> = b
    .iter_pieces()
    .map(|(sq, p)| {
        let base: u8 = match p.piece_type {
            PieceType::King     => 1,
            PieceType::Queen    => 2,
            PieceType::Rook     => 3,
            PieceType::Bishop   => 4,
            PieceType::Knight   => 5,
            PieceType::Pawn     => 6,
            PieceType::Eagle    => 7,
            PieceType::Cheetah  => 8,
            PieceType::Genie    => 9,
            PieceType::Prince   => 10,
            PieceType::Princess => 11,
        };
        let pcode = match p.color {
            Color::White => base,
            Color::Black => base + 11,
        };

        (sq.col, sq.row, pcode)
    })
    .collect();
    items.sort_unstable();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (c, r, p) in items {
        h ^= c as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
        h ^= r as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
        h ^= p as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
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

/// Рисует 4 угловых контура (скобки) в клетке.
fn draw_cell_corners(
    pixmap: &mut Pixmap,
    x: f32, y: f32,
    cell: f32,
    color: (u8, u8, u8),
                     thickness: f32,
) {
    let mut paint = Paint::default();
    paint.set_color(SkColor::from_rgba8(color.0, color.1, color.2, 255));
    let mut stroke = Stroke::default();
    stroke.width = thickness; // <-- это толщина
    stroke.line_cap = tiny_skia::LineCap::Round;

    // Длина уголка (было 25%, стало 12.5% — в 2 раза меньше).
    let len = cell * 0.125;
    // Отступ от края (было 2.0, можно оставить или чуть увеличить).
    let inset = 2.0;

    let x0 = x + inset;
    let y0 = y + inset;
    let x1 = x + cell - inset;
    let y1 = y + cell - inset;

    // Левый верхний угол.
    let mut pb = PathBuilder::new();
    pb.move_to(x0, y0 + len);
    pb.line_to(x0, y0);
    pb.line_to(x0 + len, y0);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    // Правый верхний угол.
    let mut pb = PathBuilder::new();
    pb.move_to(x1 - len, y0);
    pb.line_to(x1, y0);
    pb.line_to(x1, y0 + len);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    // Левый нижний угол.
    let mut pb = PathBuilder::new();
    pb.move_to(x0, y1 - len);
    pb.line_to(x0, y1);
    pb.line_to(x0 + len, y1);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    // Правый нижний угол.
    let mut pb = PathBuilder::new();
    pb.move_to(x1 - len, y1);
    pb.line_to(x1, y1);
    pb.line_to(x1, y1 - len);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

