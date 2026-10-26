//! Отрисовка оверлеев: превращение, ничья, отключение, ожидание,
//! кнопка «Настройки» после конца партии, транзитное сообщение.

use crate::app::{App, DisconnectKind};
//use crate::app::render_board::{
//   color_label, draw_button_label, fill_rect_rgba, outline_rect_rgba, pick_atlas,
//};

use crate::app::render_board::{ fill_rect_rgba, outline_rect_rgba, pick_atlas };
use crate::app::render_buttons::{ color_label, draw_button_label };

use chess_logic::PieceType;
use std::time::Instant;
use tiny_skia::{
    Color as SkColor, PixmapPaint, Transform,
};

impl App {
    // ============================================================
    // ПРЕВРАЩЕНИЕ
    // ============================================================

    pub(super) fn draw_promotion_overlay(&mut self) {
        let Some(promo) = self.promotion_pending else { return };

        fill_rect_rgba(
            &mut self.pixmap,
            0.0, 0.0,
            self.width as f32, self.height as f32,
            0, 0, 0, 140,
        );

        let buttons = self.promotion_buttons(promo);

        if let (Some(first), Some(last)) = (buttons.first(), buttons.last()) {
            let pad = 20.0;
            let title_h = 34.0;
            let bg_x = first.1.x - pad;
            let bg_y = first.1.y - pad - title_h;
            let bg_w = (last.1.x + last.1.w) - first.1.x + 2.0 * pad;
            let bg_h = first.1.h + 2.0 * pad + title_h;

            fill_rect_rgba(&mut self.pixmap, bg_x, bg_y, bg_w, bg_h, 40, 40, 60, 255);
            outline_rect_rgba(
                &mut self.pixmap, bg_x, bg_y, bg_w, bg_h, (200, 200, 255), 3.0,
            );

            let title = "Превращение";
            let face_size = 20.0;
            let tw = self.text.text_width(title, face_size);
            self.text.draw_text(
                &mut self.pixmap,
                title,
                bg_x + (bg_w - tw) / 2.0,
                                bg_y + 26.0,
                                face_size,
                                SkColor::from_rgba8(255, 255, 255, 255),
            );
        }

        for (pt, r) in &buttons {
            fill_rect_rgba(&mut self.pixmap, r.x, r.y, r.w, r.h, 220, 220, 220, 255);
            outline_rect_rgba(&mut self.pixmap, r.x, r.y, r.w, r.h, (0, 0, 0), 2.0);

            if let Some(img) = pick_atlas(&self.atlas, promo.color, *pt, false) {
                let iw = img.width() as f32;
                let ih = img.height() as f32;
                let cx = r.x + r.w / 2.0;
                let cy = r.y + r.h / 2.0;
                let paint = PixmapPaint {
                    opacity: 1.0,
                    blend_mode: tiny_skia::BlendMode::SourceOver,
                    quality: tiny_skia::FilterQuality::Nearest,
                };
                self.pixmap.draw_pixmap(
                    (cx - iw / 2.0) as i32,
                                        (cy - ih / 2.0) as i32,
                                        img.as_ref(),
                                        &paint,
                                        Transform::identity(),
                                        None,
                );
            }
        }
    }

    // ============================================================
    // ОВЕРЛЕЙ НИЧЬЕЙ
    // ============================================================

    pub(super) fn draw_draw_offer_overlay(&mut self) {
        let Some(by) = self.draw_offer_from else { return };

        let (accept, decline) = self.draw_overlay_buttons();

        let msg = format!("{} предлагают ничью", color_label(by));
        let face_size = 14.0;
        let msg_w = self.text.text_width(&msg, face_size);
        let pad_x = 10.0;
        let pad_y = 6.0;

        let block_w = (accept.w + 2.0 * pad_x).max(msg_w + 2.0 * pad_x);
        let box_x = accept.x - pad_x - (block_w - accept.w) / 2.0;
        let box_h = face_size + 2.0 * pad_y;
        let box_y = accept.y - box_h - 12.0;

        fill_rect_rgba(&mut self.pixmap, box_x, box_y, block_w, box_h, 40, 40, 60, 240);
        outline_rect_rgba(
            &mut self.pixmap, box_x, box_y, block_w, box_h, (200, 200, 255), 2.0,
        );

        self.text.draw_text(
            &mut self.pixmap,
            &msg,
            box_x + (block_w - msg_w) / 2.0,
                            box_y + pad_y + face_size,
                            face_size,
                            SkColor::from_rgba8(255, 255, 255, 255),
        );

        draw_button_label(&mut self.pixmap, &mut self.text, accept, "Принять", (60, 140, 60));
        draw_button_label(&mut self.pixmap, &mut self.text, decline, "Отклонить", (160, 60, 60));

        if let Some(deadline) = self.draw_offer_deadline {
            let now = Instant::now();
            if deadline > now {
                let remaining = (deadline - now).as_secs();
                let timer_label = format!("{} с", remaining);
                let timer_w = self.text.text_width(&timer_label, 12.0);
                let timer_x = decline.x + decline.w / 2.0 - timer_w / 2.0;
                let timer_y = decline.y + decline.h + 18.0;
                self.text.draw_text(
                    &mut self.pixmap,
                    &timer_label,
                    timer_x,
                    timer_y,
                    12.0,
                    SkColor::from_rgba8(180, 180, 200, 255),
                );
            }
        }
    }

    // ============================================================
    // ОВЕРЛЕЙ ОТКЛЮЧЕНИЯ
    // ============================================================

    pub(super) fn draw_disconnect_overlay(&mut self) {
        let Some(overlay) = self.disconnect_overlay.as_ref() else { return };

        fill_rect_rgba(
            &mut self.pixmap,
            0.0, 0.0,
            self.width as f32, self.height as f32,
            0, 0, 0, 180,
        );

        let board_cx = self.layout.left() + (self.layout.board_w as f32) / 2.0;
        let board_cy = self.layout.top() + (self.layout.board_h as f32) / 2.0;

        let title_color: (u8, u8, u8) = match overlay.kind {
            DisconnectKind::OpponentLeft => (255, 180, 100),
            DisconnectKind::ConnectionLost => (255, 100, 100),
        };

        let title_size = 28.0;
        let sub_size = 16.0;
        let pad = 24.0;

        let title_w = self.text.text_width(&overlay.title, title_size);
        let sub_w = self.text.text_width(&overlay.subtitle, sub_size);
        let max_w = title_w.max(sub_w);

        let box_w = max_w + 2.0 * pad;
        let box_h = title_size + sub_size + 3.0 * pad + 20.0;
        let box_x = board_cx - box_w / 2.0;
        let box_y = board_cy - box_h / 2.0 - 20.0;

        fill_rect_rgba(&mut self.pixmap, box_x, box_y, box_w, box_h, 30, 30, 45, 240);
        outline_rect_rgba(
            &mut self.pixmap, box_x, box_y, box_w, box_h, title_color, 3.0,
        );

        self.text.draw_text(
            &mut self.pixmap,
            &overlay.title,
            board_cx - title_w / 2.0,
            box_y + pad + title_size,
            title_size,
            SkColor::from_rgba8(title_color.0, title_color.1, title_color.2, 255),
        );

        self.text.draw_text(
            &mut self.pixmap,
            &overlay.subtitle,
            board_cx - sub_w / 2.0,
            box_y + 2.0 * pad + title_size + sub_size,
            sub_size,
            SkColor::from_rgba8(200, 200, 220, 255),
        );
    }

    // ============================================================
    // КНОПКА «НАСТРОЙКИ» ПОСЛЕ КОНЦА ПАРТИИ
    // ============================================================

    pub(super) fn draw_game_over_button(&mut self) {
        let btn = self.game_over_button_rect();
        draw_button_label(
            &mut self.pixmap,
            &mut self.text,
            btn,
            "Настройки",
            (60, 100, 160),
        );
    }

    // ============================================================
    // ОЖИДАНИЕ СОПЕРНИКА
    // ============================================================

    pub(super) fn draw_waiting_overlay(&mut self) {
        fill_rect_rgba(
            &mut self.pixmap,
            0.0, 0.0,
            self.width as f32, self.height as f32,
            0, 0, 0, 200,
        );

        let board_cx = self.layout.left() + (self.layout.board_w as f32) / 2.0;
        let board_cy = self.layout.top() + (self.layout.board_h as f32) / 2.0;

        let title = "Поиск соперника…";
        let title_size = 28.0;
        let title_w = self.text.text_width(title, title_size);

        let sub = "Ожидание игрока с вашим контролем времени";
        let sub_size = 15.0;
        let sub_w = self.text.text_width(sub, sub_size);

        let pad = 24.0;
        let max_w = title_w.max(sub_w);
        let box_w = max_w + 2.0 * pad;
        let box_h = title_size + sub_size + 3.0 * pad + 20.0;
        let box_x = board_cx - box_w / 2.0;
        let box_y = board_cy - box_h / 2.0 - 20.0;

        fill_rect_rgba(&mut self.pixmap, box_x, box_y, box_w, box_h, 30, 30, 45, 240);
        outline_rect_rgba(
            &mut self.pixmap, box_x, box_y, box_w, box_h, (150, 200, 255), 3.0,
        );

        self.text.draw_text(
            &mut self.pixmap,
            title,
            board_cx - title_w / 2.0,
            box_y + pad + title_size,
            title_size,
            SkColor::from_rgba8(200, 220, 255, 255),
        );

        self.text.draw_text(
            &mut self.pixmap,
            sub,
            board_cx - sub_w / 2.0,
            box_y + 2.0 * pad + title_size + sub_size,
            sub_size,
            SkColor::from_rgba8(180, 180, 200, 255),
        );

        let btn = self.waiting_cancel_button_rect();
        draw_button_label(
            &mut self.pixmap,
            &mut self.text,
            btn,
            "Отменить поиск",
            (160, 60, 60),
        );
    }

    // ============================================================
    // ТРАНЗИТНОЕ СООБЩЕНИЕ
    // ============================================================

    pub(super) fn draw_transient_message(&mut self, msg: &str) {
        let face_size = 16.0;
        let text_w = self.text.text_width(msg, face_size);
        let pad = 12.0;
        let box_w = text_w + 2.0 * pad;
        let box_h = face_size + 2.0 * pad;

        let cx = self.width as f32 / 2.0;
        let box_x = cx - box_w / 2.0;
        let box_y = 20.0;

        fill_rect_rgba(&mut self.pixmap, box_x, box_y, box_w, box_h, 90, 40, 40, 240);
        outline_rect_rgba(
            &mut self.pixmap, box_x, box_y, box_w, box_h, (255, 150, 150), 2.0,
        );
        self.text.draw_text(
            &mut self.pixmap,
            msg,
            box_x + pad,
            box_y + pad + face_size,
            face_size,
            SkColor::from_rgba8(255, 255, 255, 255),
        );
    }

    // ============================================================
    // SETUP: оверлеи лобби (приглашение / ожидание)
    // ============================================================

    pub(super) fn draw_setup_overlay(&mut self) {
        use crate::setup::LobbyView;

        match self.setup.lobby.clone() {
            LobbyView::Idle => {}
            LobbyView::InviteSent { to_name } => {
                self.draw_setup_waiting_modal(
                    "Ожидание ответа",
                    &format!("Ждём подтверждения от {to_name}…"),
                                              Some("Отменить"),
                );
            }
            LobbyView::InviteReceived {
                from_name,
                variant,
                time_control,
            } => {
                self.draw_setup_invite_modal(&from_name, &variant, &time_control);
            }
            LobbyView::InPair { partner_name } => {
                self.draw_setup_waiting_modal(
                    "Команда сформирована",
                    &format!("Партнёр: {partner_name}. Ищем соперников…"),
                                              Some("Отменить"),
                );
            }
        }
    }

    fn draw_setup_waiting_modal(
        &mut self,
        title: &str,
        subtitle: &str,
        button: Option<&str>,
    ) {
        let w = self.width as f32;
        let h = self.height as f32;
        fill_rect_rgba(&mut self.pixmap, 0.0, 0.0, w, h, 0, 0, 0, 160);

        let cx = w / 2.0;
        let cy = h / 2.0;
        let title_size = 24.0;
        let sub_size = 16.0;
        let pad = 24.0;
        let title_w = self.text.text_width(title, title_size);
        let sub_w = self.text.text_width(subtitle, sub_size);
        let box_w = (title_w.max(sub_w)) + 2.0 * pad;
        let box_h = title_size + sub_size + 3.0 * pad + 40.0;
        let box_x = cx - box_w / 2.0;
        let box_y = cy - box_h / 2.0;

        fill_rect_rgba(&mut self.pixmap, box_x, box_y, box_w, box_h, 30, 30, 45, 240);
        outline_rect_rgba(&mut self.pixmap, box_x, box_y, box_w, box_h, (150, 200, 255), 3.0);

        self.text.draw_text(
            &mut self.pixmap,
            title,
            cx - title_w / 2.0,
            box_y + pad + title_size,
            title_size,
            SkColor::from_rgba8(200, 220, 255, 255),
        );
        self.text.draw_text(
            &mut self.pixmap,
            subtitle,
            cx - sub_w / 2.0,
            box_y + 2.0 * pad + title_size + sub_size,
            sub_size,
            SkColor::from_rgba8(180, 180, 200, 255),
        );

        if let Some(label) = button {
            let btn_w = 160.0;
            let btn_h = 44.0;
            let btn = crate::setup::Rect {
                x: cx - btn_w / 2.0,
                y: box_y + box_h - pad - btn_h,
                w: btn_w,
                h: btn_h,
            };
            draw_button_label(&mut self.pixmap, &mut self.text, btn, label, (160, 60, 60));
        }
    }

    fn draw_setup_invite_modal(
        &mut self,
        from_name: &str,
        variant: &str,
        time_control: &str,
    ) {
        let w = self.width as f32;
        let h = self.height as f32;
        fill_rect_rgba(&mut self.pixmap, 0.0, 0.0, w, h, 0, 0, 0, 180);

        let cx = w / 2.0;
        let cy = h / 2.0;

        let title = "Приглашение в команду";
        let msg = format!("{from_name} приглашает вас в команду");
        let sub = format!("Режим: 2×2, вариант {variant}, время {time_control}");

        let title_size = 22.0;
        let msg_size = 16.0;
        let sub_size = 14.0;
        let pad = 24.0;

        let title_w = self.text.text_width(title, title_size);
        let msg_w = self.text.text_width(&msg, msg_size);
        let sub_w = self.text.text_width(&sub, sub_size);
        let box_w = (title_w.max(msg_w).max(sub_w)) + 2.0 * pad;
        let box_h = title_size + msg_size + sub_size + 4.0 * pad + 60.0;
        let box_x = cx - box_w / 2.0;
        let box_y = cy - box_h / 2.0;

        fill_rect_rgba(&mut self.pixmap, box_x, box_y, box_w, box_h, 30, 30, 45, 245);
        outline_rect_rgba(&mut self.pixmap, box_x, box_y, box_w, box_h, (150, 220, 150), 3.0);

        let mut y = box_y + pad + title_size;
        self.text.draw_text(
            &mut self.pixmap,
            title,
            cx - title_w / 2.0,
            y,
            title_size,
            SkColor::from_rgba8(200, 220, 255, 255),
        );
        y += pad + msg_size;
        self.text.draw_text(
            &mut self.pixmap,
            &msg,
            cx - msg_w / 2.0,
            y,
            msg_size,
            SkColor::from_rgba8(230, 230, 240, 255),
        );
        y += pad + sub_size;
        self.text.draw_text(
            &mut self.pixmap,
            &sub,
            cx - sub_w / 2.0,
            y,
            sub_size,
            SkColor::from_rgba8(180, 180, 200, 255),
        );

        let btn_w = 150.0;
        let btn_h = 44.0;
        let btn_gap = 12.0;
        let accept = crate::setup::Rect {
            x: cx - btn_w - btn_gap / 2.0,
            y: box_y + box_h - pad - btn_h,
            w: btn_w,
            h: btn_h,
        };
        let decline = crate::setup::Rect {
            x: cx + btn_gap / 2.0,
            y: box_y + box_h - pad - btn_h,
            w: btn_w,
            h: btn_h,
        };

        draw_button_label(&mut self.pixmap, &mut self.text, accept, "Принять", (60, 140, 60));
        draw_button_label(&mut self.pixmap, &mut self.text, decline, "Отклонить", (160, 60, 60));
    }

    // ============================================================
    // 2×2: запрос партнёра (фаза 1) — баннер слева
    // ============================================================

    pub(super) fn draw_draw_request_banner(&mut self) {
        let Some(from_name) = self.draw_request_from_partner.clone() else { return };

        // Баннер слева, под кнопками.
        let panel_cx = (crate::layout::LEFT_PANEL as f32) / 2.0;
        let (accept, decline) = self.draw_request_buttons();

        let title = "Партнёр предлагает ничью";
        let sub = format!("{from_name} просит подтвердить");
        let title_size = 14.0;
        let sub_size = 13.0;
        let pad = 10.0;
        let title_w = self.text.text_width(title, title_size);
        let sub_w = self.text.text_width(&sub, sub_size);
        let box_w = (accept.w).max(title_w).max(sub_w) + 2.0 * pad;
        let box_h = title_size + sub_size + 3.0 * pad + accept.h * 2.0 + 8.0;
        let box_x = panel_cx - box_w / 2.0;
        let box_y = accept.y - box_h - 12.0;

        fill_rect_rgba(&mut self.pixmap, box_x, box_y, box_w, box_h, 40, 40, 60, 240);
        outline_rect_rgba(&mut self.pixmap, box_x, box_y, box_w, box_h, (255, 200, 100), 2.0);

        self.text.draw_text(
            &mut self.pixmap,
            title,
            box_x + pad,
            box_y + pad + title_size,
            title_size,
            SkColor::from_rgba8(255, 220, 150, 255),
        );
        self.text.draw_text(
            &mut self.pixmap,
            &sub,
            box_x + pad,
            box_y + 2.0 * pad + title_size + sub_size,
            sub_size,
            SkColor::from_rgba8(220, 220, 240, 255),
        );

        draw_button_label(&mut self.pixmap, &mut self.text, accept, "Подтвердить", (60, 140, 60));
        draw_button_label(&mut self.pixmap, &mut self.text, decline, "Отказать", (160, 60, 60));

        // Таймер.
        if let Some(deadline) = self.draw_request_deadline {
            let now = Instant::now();
            if deadline > now {
                let remaining = (deadline - now).as_secs();
                let timer_label = format!("{} с", remaining);
                let tw = self.text.text_width(&timer_label, 12.0);
                self.text.draw_text(
                    &mut self.pixmap,
                    &timer_label,
                    panel_cx - tw / 2.0,
                    decline.y + decline.h + 18.0,
                    12.0,
                    SkColor::from_rgba8(180, 180, 200, 255),
                );
            }
        }
    }

    // ============================================================
    // 2×2: ничья соперников (фаза 2) — оверлей над доской
    // ============================================================

    pub(super) fn draw_draw_offer_2v2_overlay(&mut self) {
        let Some(offer) = self.draw_offer_2v2.clone() else { return };

        let board_cx = self.layout.left() + (self.layout.board_w as f32) / 2.0;
        let board_cy = self.layout.top() + (self.layout.board_h as f32) / 2.0;

        // Затемнение только над доской (не весь экран — чтобы баннер не мешал).
        fill_rect_rgba(
            &mut self.pixmap,
            self.layout.left(),
                       self.layout.top(),
                       self.layout.board_w as f32,
                       self.layout.board_h as f32,
                       0, 0, 0, 140,
        );

        let names = format!("{} и {}", offer.by_names[0], offer.by_names[1]);
        let title = format!("{} предлагают ничью", names);
        let title_size = 20.0;
        let tw = self.text.text_width(&title, title_size);

        let pad = 24.0;
        let box_w = tw + 2.0 * pad;
        let box_h = 140.0;
        let box_x = board_cx - box_w / 2.0;
        let box_y = board_cy - box_h / 2.0;

        fill_rect_rgba(&mut self.pixmap, box_x, box_y, box_w, box_h, 30, 30, 45, 245);
        outline_rect_rgba(&mut self.pixmap, box_x, box_y, box_w, box_h, (150, 220, 150), 3.0);

        self.text.draw_text(
            &mut self.pixmap,
            &title,
            board_cx - tw / 2.0,
            box_y + pad + title_size,
            title_size,
            SkColor::from_rgba8(200, 230, 200, 255),
        );

        if offer.accepted_by_me {
            // Уже приняли — ждём второго соперника.
            let sub = "ждём второго соперника…";
            let sw = self.text.text_width(sub, 15.0);
            self.text.draw_text(
                &mut self.pixmap,
                sub,
                board_cx - sw / 2.0,
                box_y + 2.0 * pad + title_size + 20.0,
                15.0,
                SkColor::from_rgba8(180, 180, 200, 255),
            );
        } else {
            let (accept, decline) = self.draw_offer_2v2_buttons();
            draw_button_label(&mut self.pixmap, &mut self.text, accept, "Принять", (60, 140, 60));
            draw_button_label(&mut self.pixmap, &mut self.text, decline, "Отклонить", (160, 60, 60));

            // Таймер.
            let now = Instant::now();
            if offer.deadline > now {
                let remaining = (offer.deadline - now).as_secs();
                let timer_label = format!("{} с", remaining);
                let tw2 = self.text.text_width(&timer_label, 12.0);
                self.text.draw_text(
                    &mut self.pixmap,
                    &timer_label,
                    board_cx - tw2 / 2.0,
                    box_y + box_h - pad,
                    12.0,
                    SkColor::from_rgba8(180, 180, 200, 255),
                );
            }
        }
    }

    pub(super) fn draw_resign_request_banner(&mut self) {
        let Some(from_name) = self.resign_request_from_partner.clone() else { return };

        // Баннер слева, под кнопками (используем ту же геометрию, что для ничьей).
        let panel_cx = (crate::layout::LEFT_PANEL as f32) / 2.0;
        let (accept, decline) = self.draw_request_buttons();

        let title = "Партнёр предлагает сдаться";
        let sub = format!("{from_name} просит подтвердить");
        let title_size = 14.0;
        let sub_size = 13.0;
        let pad = 10.0;
        let title_w = self.text.text_width(title, title_size);
        let sub_w = self.text.text_width(&sub, sub_size);
        let box_w = (accept.w).max(title_w).max(sub_w) + 2.0 * pad;
        let box_h = title_size + sub_size + 3.0 * pad + accept.h * 2.0 + 8.0;
        let box_x = panel_cx - box_w / 2.0;
        let box_y = accept.y - box_h - 12.0;

        fill_rect_rgba(&mut self.pixmap, box_x, box_y, box_w, box_h, 60, 30, 30, 240);
        outline_rect_rgba(&mut self.pixmap, box_x, box_y, box_w, box_h, (255, 150, 150), 2.0);

        self.text.draw_text(
            &mut self.pixmap,
            title,
            box_x + pad,
            box_y + pad + title_size,
            title_size,
            SkColor::from_rgba8(255, 200, 200, 255),
        );
        self.text.draw_text(
            &mut self.pixmap,
            &sub,
            box_x + pad,
            box_y + 2.0 * pad + title_size + sub_size,
            sub_size,
            SkColor::from_rgba8(220, 220, 240, 255),
        );

        draw_button_label(&mut self.pixmap, &mut self.text, accept, "Сдаться", (160, 60, 60));
        draw_button_label(&mut self.pixmap, &mut self.text, decline, "Отказать", (60, 100, 160));

        // Таймер.
        if let Some(deadline) = self.resign_request_deadline {
            let now = Instant::now();
            if deadline > now {
                let remaining = (deadline - now).as_secs();
                let timer_label = format!("{} с", remaining);
                let tw = self.text.text_width(&timer_label, 12.0);
                self.text.draw_text(
                    &mut self.pixmap,
                    &timer_label,
                    panel_cx - tw / 2.0,
                    decline.y + decline.h + 18.0,
                    12.0,
                    SkColor::from_rgba8(180, 180, 200, 255),
                );
            }
        }
    }

}
