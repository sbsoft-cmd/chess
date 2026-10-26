//! Обработка ввода, сетевых событий, ходов, превращения, ничьей,
//! отключения и бота. Всё, что меняет состояние App в ответ на события.

use crate::app::{
    App, ButtonKind, DisconnectKind, DisconnectOverlay, Phase, PromotionPending,
    ReviewAction, TRANSIENT_MSG_LIFETIME,
};
use crate::layout::{Layout, MARGIN};
use chess_client_net::{ClientCommand, ClientEvent};
use chess_logic::{Color, GameMode, Piece, PieceType, Seat, Square};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tracing::{info, warn};
use winit::event::ElementState;
use winit::keyboard::{Key, NamedKey};

#[cfg(debug_assertions)]
use crate::app::BotMove;

impl App {
    // --- ввод: setup ---

    pub(crate) fn draw_request_buttons(&self) -> (crate::setup::Rect, crate::setup::Rect) {
        let panel_cx = (crate::layout::LEFT_PANEL as f32) / 2.0;
        let w = 160.0;
        let h = 38.0;
        let gap = 8.0;
        let base_y = (self.height as f32) * 0.75;
        let accept = crate::setup::Rect { x: panel_cx - w / 2.0, y: base_y, w, h };
        let decline = crate::setup::Rect { x: panel_cx - w / 2.0, y: base_y + h + gap, w, h };
        (accept, decline)
    }

    pub(crate) fn draw_offer_2v2_buttons(&self) -> (crate::setup::Rect, crate::setup::Rect) {
        let board_cx = self.layout.left() + (self.layout.board_w as f32) / 2.0;
        let board_cy = self.layout.top() + (self.layout.board_h as f32) / 2.0;
        let w = 160.0;
        let h = 44.0;
        let gap = 12.0;
        let accept = crate::setup::Rect { x: board_cx - w - gap / 2.0, y: board_cy + 40.0, w, h };
        let decline = crate::setup::Rect { x: board_cx + gap / 2.0, y: board_cy + 40.0, w, h };
        (accept, decline)
    }

    pub(crate) fn review_buttons(&self) -> Vec<(ReviewAction, crate::setup::Rect)> {
        let panel_cx = (crate::layout::LEFT_PANEL as f32) / 2.0;
        let size = 30.0;
        let gap = 6.0;
        let count = 4;
        let total_w = count as f32 * size + (count as f32 - 1.0) * gap;
        let start_x = panel_cx - total_w / 2.0;
        let board_bottom = self.layout.bottom();
        let y = board_bottom + 10.0;
        let actions = [ReviewAction::Start, ReviewAction::Prev, ReviewAction::Next, ReviewAction::End];
        actions.iter().enumerate().map(|(i, act)| {
            let x = start_x + i as f32 * (size + gap);
            (*act, crate::setup::Rect { x, y, w: size, h: size })
        }).collect()
    }

    pub(crate) fn handle_setup_key(&mut self, event: &winit::event::KeyEvent) {
        if event.state != ElementState::Pressed { return; }
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => { self.setup.pop_char(); }
            Key::Named(NamedKey::Enter) => {
                if self.setup.can_start() { self.start_game(); }
            }
            Key::Named(NamedKey::Space) => { self.setup.push_char(' '); }
            Key::Character(s) => {
                for ch in s.chars() { self.setup.push_char(ch); }
            }
            _ => {}
        }
    }

    pub(crate) fn handle_setup_click(&mut self, px: f32, py: f32) {
        if self.setup_layout.name_field.contains(px, py) {
            self.setup.focused_field = crate::setup::FocusField::Name;
            return;
        }
        if self.setup_layout.opponent_field.contains(px, py) {
            self.setup.focused_field = crate::setup::FocusField::Opponent;
            return;
        }
        if self.setup_layout.partner_field.contains(px, py) {
            self.setup.focused_field = crate::setup::FocusField::Partner;
            return;
        }

        if self.setup_layout.variant_8.contains(px, py) {
            self.setup.variant = crate::setup::VARIANT_8X8;
            self.setup.focused_field = crate::setup::FocusField::None;
            return;
        }
        if self.setup_layout.variant_8_free.contains(px, py) {
            self.setup.variant = crate::setup::VARIANT_8X8_FREESTYLE;
            self.setup.focused_field = crate::setup::FocusField::None;
            return;
        }
        if self.setup_layout.variant_10.contains(px, py) {
            self.setup.variant = crate::setup::VARIANT_10X10;
            self.setup.focused_field = crate::setup::FocusField::None;
            return;
        }
        if self.setup_layout.variant_10_free.contains(px, py) {
            self.setup.variant = crate::setup::VARIANT_10X10_FREESTYLE;
            self.setup.focused_field = crate::setup::FocusField::None;
            return;
        }
        if self.setup_layout.variant_16.contains(px, py) {
            self.setup.variant = crate::setup::VARIANT_16X10;
            self.setup.focused_field = crate::setup::FocusField::None;
            return;
        }

        for (i, r) in self.setup_layout.time_rects.iter().enumerate() {
            if r.contains(px, py) {
                if let Some(opt) = crate::setup::TIME_OPTIONS.get(i) {
                    self.setup.time_control = opt.value;
                }
                self.setup.focused_field = crate::setup::FocusField::None;
                return;
            }
        }

        #[cfg(debug_assertions)]
        {
            if self.setup_layout.role_human.contains(px, py) {
                self.setup.role = crate::setup::Role::Human;
                self.setup.focused_field = crate::setup::FocusField::None;
                return;
            }
            if self.setup_layout.role_bot.contains(px, py) {
                self.setup.role = crate::setup::Role::Bot;
                self.setup.focused_field = crate::setup::FocusField::None;
                return;
            }
        }

        if self.setup_layout.play_button.contains(px, py) {
            if self.setup.can_start() { self.start_game(); }
            return;
        }

        self.setup.focused_field = crate::setup::FocusField::None;
    }

    // --- ввод: playing ---

    pub(crate) fn handle_press_playing(&mut self, px: f32, py: f32) {
        if self.draw_request_from_partner.is_some() {
            let (accept_rect, decline_rect) = self.draw_request_buttons();
            if accept_rect.contains(px, py) {
                if let Some(net) = self.net.as_ref() { let _ = net.cmd_tx.send(ClientCommand::OfferDrawConfirm); }
                self.draw_request_from_partner = None;
                self.draw_request_deadline = None;
                self.bg_key = None;
                if let Some(w) = &self.window { w.request_redraw(); }
                return;
            }
            if decline_rect.contains(px, py) {
                if let Some(net) = self.net.as_ref() { let _ = net.cmd_tx.send(ClientCommand::OfferDrawDecline); }
                self.draw_request_from_partner = None;
                self.draw_request_deadline = None;
                self.bg_key = None;
                if let Some(w) = &self.window { w.request_redraw(); }
                return;
            }
        }

        if let Some(ref offer) = self.draw_offer_2v2 {
            if !offer.accepted_by_me {
                let (accept_rect, decline_rect) = self.draw_offer_2v2_buttons();
                if accept_rect.contains(px, py) {
                    if let Some(net) = self.net.as_ref() { let _ = net.cmd_tx.send(ClientCommand::AcceptDraw); }
                    return;
                }
                if decline_rect.contains(px, py) {
                    if let Some(net) = self.net.as_ref() { let _ = net.cmd_tx.send(ClientCommand::DeclineDraw); }
                    self.draw_offer_2v2 = None;
                    self.bg_key = None;
                    if let Some(w) = &self.window { w.request_redraw(); }
                    return;
                }
            }
        }

        if self.resign_request_from_partner.is_some() {
            let (accept_rect, decline_rect) = self.draw_request_buttons();
            if accept_rect.contains(px, py) {
                if let Some(net) = self.net.as_ref() { let _ = net.cmd_tx.send(ClientCommand::OfferResignConfirm); }
                self.resign_request_from_partner = None;
                self.resign_request_deadline = None;
                self.bg_key = None;
                if let Some(w) = &self.window { w.request_redraw(); }
                return;
            }
            if decline_rect.contains(px, py) {
                if let Some(net) = self.net.as_ref() { let _ = net.cmd_tx.send(ClientCommand::OfferResignDecline); }
                self.resign_request_from_partner = None;
                self.resign_request_deadline = None;
                self.bg_key = None;
                if let Some(w) = &self.window { w.request_redraw(); }
                return;
            }
        }

        if self.state.as_ref().map_or(false, |st| st.is_over()) && !self.history.is_empty() {
            for (action, rect) in self.review_buttons() {
                if rect.contains(px, py) {
                    match action {
                        ReviewAction::Start => {
                            self.review_index = Some(0);
                            self.clear_selection();
                            self.clear_reserve_selection();
                            self.promotion_pending = None;
                            self.dragging = false;
                            self.drag_from = None;
                            self.dragging_reserve = false;
                            self.drag_from_reserve = None;
                            self.dragging_hidden_royal = None;
                        }
                        ReviewAction::Prev => {
                            if let Some(i) = self.review_index {
                                if i > 0 { self.review_index = Some(i - 1); }
                            } else {
                                self.review_index = Some(self.history.len() - 1);
                            }
                        }
                        ReviewAction::Next => {
                            if let Some(i) = self.review_index {
                                if i + 1 < self.history.len() { self.review_index = Some(i + 1); }
                                else { self.review_index = None; }
                            }
                        }
                        ReviewAction::End => self.review_index = Some(self.history.len() - 1),
                    }
                    self.update_review_last_move();
                    self.bg_key = None;
                    if let Some(w) = &self.window { w.request_redraw(); }
                    return;
                }
            }
        }

        if self.waiting_for_opponent {
            if self.waiting_cancel_button_at(px, py) { self.return_to_setup(); }
            return;
        }

        if self.disconnect_overlay.is_some() {
            if self.game_over_button_at(px, py) { self.return_to_setup(); }
            return;
        }

        if self.game_over_button_at(px, py) {
            self.return_to_setup();
            return;
        }

        if let Some(promo) = self.promotion_pending {
            if let Some(pt) = self.promotion_button_at(px, py) {
                self.apply_promotion_choice(promo, pt);
            }
            return;
        }

        if self.draw_offer_from.is_some() {
            if let Some(accept) = self.draw_overlay_button_at(px, py) {
                self.handle_draw_decision(accept);
                return;
            }
        }

        if let Some(btn) = self.button_at(px, py) {
            self.handle_button_click(btn.kind);
            if let Some(w) = &self.window { w.request_redraw(); }
            return;
        }

        if !self.my_hidden_royals.is_empty() {
            let panel_x = self.layout.right() + MARGIN as f32;
            let slot_size = self.layout.cell as f32 * 0.8;
            let gap = 10.0;
            let mut cy = self.layout.top() + slot_size / 2.0;
            for color in [Color::White, Color::Black] {
                let team = match color {
                    Color::White => &self.state.as_ref().unwrap().white_team,
                    Color::Black => &self.state.as_ref().unwrap().black_team,
                };
                for (seat_idx, seat) in [Seat::First, Seat::Second].iter().enumerate() {
                    let p = &team.players[seat_idx];
                    for pt in [PieceType::Prince, PieceType::Princess] {
                        if p.is_in_holding(pt) {
                            let x = panel_x + slot_size / 2.0;
                            let rect = crate::setup::Rect {
                                x: x - slot_size / 2.0, y: cy - slot_size / 2.0, w: slot_size, h: slot_size,
                            };
                            if rect.contains(px, py) {
                                if p.get_cd(pt) == 0
                                    && Some(color) == self.my_color
                                    && (self.my_seat.is_none() || Some(*seat) == self.my_seat)
                                    {
                                        self.dragging_hidden_royal = Some((pt, *seat));
                                        self.drag_pixel_x = px;
                                        self.drag_pixel_y = py;
                                        if let Some(w) = &self.window { w.request_redraw(); }
                                    }
                                    return;
                            }
                            cy += slot_size + gap;
                        }
                    }
                }
            }
            return;
        }

        if self.state.as_ref().map_or(true, |st| st.mode == GameMode::Duel) {
            if let Some(slot) = self.reserve_slot_at(px, py) {
                let (color, pt) = (slot.color, slot.piece_type);
                self.on_reserve_click(color, pt);
                if let Some(w) = &self.window { w.request_redraw(); }
                return;
            }
        }

        let st = match self.state.as_ref() { Some(s) => s, None => return };
        if st.is_over() { return; }

        let cell = match self.pixel_to_cell(px, py) {
            Some(c) => c,
            None => {
                self.clear_selection();
                self.clear_reserve_selection();
                if let Some(w) = &self.window { w.request_redraw(); }
                return;
            }
        };
        let my = match self.my_color { Some(c) => c, None => return };

        if self.state.as_ref().map_or(true, |st| st.mode == GameMode::Duel) {
            if let Some((color, piece_type)) = self.selected_reserve {
                if self.drop_moves.contains(&cell) {
                    self.send_drop(color, piece_type, cell);
                    return;
                }
                self.clear_reserve_selection();
                if let Some(w) = &self.window { w.request_redraw(); }
                return;
            }
        }

        if let Some(sel) = self.selected {
            let is_castle = self.legal_castles.iter().any(|m| *m == cell);
            let is_move = self.legal_moves.iter().any(|m| *m == cell);
            if is_move || is_castle {
                self.send_move(sel, cell, is_castle);
                return;
            }
        }

        let piece = match st.board.get(cell) {
            Some(p) => p,
            None => {
                self.clear_selection();
                if let Some(w) = &self.window { w.request_redraw(); }
                return;
            }
        };
        if piece.color != my || st.current_player != my {
            self.clear_selection();
            if let Some(w) = &self.window { w.request_redraw(); }
            return;
        }

        if let (Some(st), Some(my_seat)) = (self.state.as_ref(), self.my_seat) {
            if st.mode == GameMode::Team2v2 {
                if st.board.get_owner(cell) != Some(my_seat) {
                    self.clear_selection();
                    if let Some(w) = &self.window { w.request_redraw(); }
                    return;
                }
            }
        }

        self.select_cell(cell);
        if let Some(w) = &self.window { w.request_redraw(); }
    }

    pub(crate) fn handle_button_click(&mut self, kind: ButtonKind) {
        match kind {
            ButtonKind::Flip => {
                self.board_flipped = !self.board_flipped;
                self.rebuild_reserve_slots();
                self.bg_key = None;
            }
            ButtonKind::Resign => {
                if let Some(st) = self.state.as_ref() {
                    if st.is_over() { return; }
                    if st.mode == GameMode::Team2v2 {
                        if let Some(net) = self.net.as_ref() {
                            let _ = net.cmd_tx.send(ClientCommand::OfferResignRequest);
                        }
                        self.transient_message = Some(("ждём подтверждения партнёра".into(), Instant::now() + TRANSIENT_MSG_LIFETIME));
                        self.bg_key = None;
                        if let Some(w) = &self.window { w.request_redraw(); }
                        return;
                    }
                }
                if let Some(net) = self.net.as_ref() {
                    let _ = net.cmd_tx.send(ClientCommand::Resign);
                }
            }
            ButtonKind::Draw => {
                if let Some(st) = self.state.as_ref() {
                    if st.is_over() { return; }
                    if st.mode == GameMode::Team2v2 {
                        if let Some(net) = self.net.as_ref() {
                            let _ = net.cmd_tx.send(ClientCommand::OfferDrawRequest);
                        }
                        self.transient_message = Some(("ждём подтверждения партнёра".into(), Instant::now() + TRANSIENT_MSG_LIFETIME));
                        self.bg_key = None;
                        if let Some(w) = &self.window { w.request_redraw(); }
                        return;
                    }
                }
                if let Some(net) = self.net.as_ref() {
                    let _ = net.cmd_tx.send(ClientCommand::OfferDraw);
                }
            }
        }
    }

    pub(crate) fn reserve_slot_at(&self, px: f32, py: f32) -> Option<crate::app::ReserveSlot> {
        for s in &self.reserve_slots {
            let dx = px - s.cx;
            let dy = py - s.cy;
            let half = s.size / 2.0;
            if dx.abs() <= half && dy.abs() <= half {
                return Some(*s);
            }
        }
        None
    }

    pub(crate) fn on_reserve_click(&mut self, color: Color, piece_type: PieceType) {
        let (in_reserve, current_player, is_over, drop_moves) = match self.state.as_ref() {
            Some(st) => (
                st.reserve.get(&color).map_or(false, |v| v.contains(&piece_type)),
                         st.current_player,
                         st.is_over(),
                         st.legal_drops(piece_type),
            ),
            None => return,
        };
        if is_over { return; }
        let my = match self.my_color { Some(c) => c, None => return };
        if color != my || current_player != my { return; }
        if !in_reserve { self.clear_reserve_selection(); return; }
        if self.selected_reserve == Some((color, piece_type)) {
            self.clear_reserve_selection();
            self.bg_key = None;
            return;
        }
        self.clear_selection();
        self.selected_reserve = Some((color, piece_type));
        self.drop_moves = drop_moves;
        self.bg_key = None;
    }

    pub(crate) fn select_cell(&mut self, cell: Square) {
        let (legal_moves, legal_castles, captures) = match self.state.as_ref() {
            Some(st) => {
                let lm = st.legal_moves(cell);
                let lc = st.legal_castles(cell);
                let mut cap = HashSet::new();
                for m in &lm {
                    if st.board.get(*m).is_some() { cap.insert(*m); }
                }
                (lm, lc, cap)
            }
            None => return,
        };
        self.selected = Some(cell);
        self.legal_moves = legal_moves;
        self.legal_castles = legal_castles;
        self.capture_moves = captures;
        self.clear_reserve_selection();
    }

    pub(crate) fn send_move(&mut self, from: Square, to: Square, is_castle: bool) {
        if !is_castle {
            if let Some(st) = self.state.as_ref() {
                if let Some(piece) = st.board.get(from) {
                    if piece.piece_type == PieceType::Pawn
                        && to.row == piece.color.pawn_promotion_row(st.board.rows() as i8)
                        {
                            self.promotion_pending = Some(PromotionPending {
                                from, to, color: piece.color,
                            });
                            self.clear_selection();
                            self.bg_key = None;
                            if let Some(w) = &self.window { w.request_redraw(); }
                            return;
                        }
                }
            }
        }

        // Доску локально НЕ меняем — придёт State от сервера.

        self.last_move_squares.clear();
        self.last_move_squares.push(from);
        self.last_move_squares.push(to);

        if let Some(net) = self.net.as_ref() {
            let cmd = if is_castle {
                ClientCommand::Castle { a: from, c: to }
            } else {
                ClientCommand::MakeMove { from, to, promotion: None }
            };
            if net.cmd_tx.send(cmd).is_ok() {
                self.waiting_for_state = true;
                self.waiting_for_state_since = Some(Instant::now());
                // Временно рисуем фигуру на to, чтобы не было мигания
                // до прихода State. hidden_source больше не нужен.
                self.pending_move_draw = Some((from, to));
            } else {
                warn!("[ui] не удалось отправить команду");
            }

        }
        self.clear_selection();
        self.bg_key = None;
        if let Some(w) = &self.window { w.request_redraw(); }
    }


    pub(crate) fn send_drop(&mut self, color: Color, piece_type: PieceType, to: Square) {

        self.last_move_squares.clear();
        self.last_move_squares.push(to);

        if let Some(net) = self.net.as_ref() {
            let cmd = ClientCommand::Drop { piece: piece_type, to };
            if net.cmd_tx.send(cmd).is_ok() {
                self.waiting_for_state = true;
                self.waiting_for_state_since = Some(Instant::now());
            } else {
                warn!("[ui] не удалось отправить Drop");
            }
        }
        self.clear_reserve_selection();
        self.bg_key = None;
        if let Some(w) = &self.window { w.request_redraw(); }

    }

    pub(crate) fn clear_selection(&mut self) {
        self.selected = None;
        self.legal_moves.clear();
        self.legal_castles.clear();
        self.capture_moves.clear();
    }

    pub(crate) fn clear_reserve_selection(&mut self) {
        self.selected_reserve = None;
        self.drop_moves.clear();
        self.bg_key = None;
    }

    // --- превращение ---

    pub(crate) fn promotion_button_at(&self, px: f32, py: f32) -> Option<PieceType> {
        let promo = self.promotion_pending?;
        for (pt, rect) in self.promotion_buttons(promo) {
            if rect.contains(px, py) { return Some(pt); }
        }
        None
    }

    pub(crate) fn promotion_buttons(&self, _promo: PromotionPending) -> Vec<(PieceType, crate::setup::Rect)> {
        let mode = self.state.as_ref().map(|s| s.mode).unwrap_or(chess_logic::GameMode::Duel);
        let choices: &[PieceType] = match mode {
            chess_logic::GameMode::Classic8x8 => &[
                PieceType::Queen, PieceType::Rook, PieceType::Bishop, PieceType::Knight,
            ],
            chess_logic::GameMode::Duel | chess_logic::GameMode::Team2v2 => &[
                PieceType::Queen, PieceType::Rook, PieceType::Bishop, PieceType::Knight,
                PieceType::Eagle, PieceType::Cheetah, PieceType::Genie,
            ],
        };
        let cell = self.layout.cell as f32;
        let gap = 8.0;
        let total_w = choices.len() as f32 * (cell + gap) - gap;
        let board_cx = self.layout.left() + (self.layout.board_w as f32) / 2.0;
        let board_cy = self.layout.top() + (self.layout.board_h as f32) / 2.0;
        let start_x = board_cx - total_w / 2.0;
        let y = board_cy - cell / 2.0;
        choices.iter().enumerate().map(|(i, pt)| {
            let x = start_x + i as f32 * (cell + gap);
            (*pt, crate::setup::Rect { x, y, w: cell, h: cell })
        }).collect()
    }

    pub(crate) fn apply_promotion_choice(&mut self, promo: PromotionPending, pt: PieceType) {
        self.promotion_pending = None;

        self.last_move_squares.clear();
        self.last_move_squares.push(promo.from);
        self.last_move_squares.push(promo.to);

        if let Some(net) = self.net.as_ref() {
            let cmd = ClientCommand::MakeMove {
                from: promo.from, to: promo.to, promotion: Some(pt),
            };
            if net.cmd_tx.send(cmd).is_ok() {
                self.waiting_for_state = true;
                self.waiting_for_state_since = Some(Instant::now());
                self.pending_move_draw = Some((promo.from, promo.to));
            } else {
                warn!("[ui] не удалось отправить promotion");
            }

        }
        self.bg_key = None;
        if let Some(w) = &self.window { w.request_redraw(); }
    }

    // --- ничья ---

    pub(crate) fn draw_overlay_button_at(&self, px: f32, py: f32) -> Option<bool> {
        let (accept_rect, decline_rect) = self.draw_overlay_buttons();
        if accept_rect.contains(px, py) { return Some(true); }
        if decline_rect.contains(px, py) { return Some(false); }
        None
    }

    pub(crate) fn draw_overlay_buttons(&self) -> (crate::setup::Rect, crate::setup::Rect) {
        let w = 140.0;
        let h = 38.0;
        let gap = 10.0;
        let draw_btn = self.buttons.iter().find(|b| b.kind == ButtonKind::Draw);
        let (cx, base_y) = match draw_btn {
            Some(b) => (b.cx, b.cy + b.radius + 60.0),
            None => (crate::layout::LEFT_PANEL as f32 / 2.0, 500.0),
        };
        let accept = crate::setup::Rect { x: cx - w / 2.0, y: base_y, w, h };
        let decline = crate::setup::Rect { x: cx - w / 2.0, y: base_y + h + gap, w, h };
        (accept, decline)
    }

    pub(crate) fn handle_draw_decision(&mut self, accept: bool) {
        if let Some(net) = self.net.as_ref() {
            let cmd = if accept { ClientCommand::AcceptDraw } else { ClientCommand::DeclineDraw };
            let _ = net.cmd_tx.send(cmd);
        }
        self.draw_offer_from = None;
        self.draw_offer_deadline = None;
        self.bg_key = None;
        if let Some(w) = &self.window { w.request_redraw(); }
    }

    // --- отключение ---

    pub(crate) fn disconnect_button_at(&self, px: f32, py: f32) -> Option<bool> {
        self.disconnect_overlay.as_ref()?;
        let rect = self.disconnect_button_rect();
        if rect.contains(px, py) { Some(true) } else { None }
    }

    pub(crate) fn disconnect_button_rect(&self) -> crate::setup::Rect {
        let board_cx = self.layout.left() + (self.layout.board_w as f32) / 2.0;
        let board_cy = self.layout.top() + (self.layout.board_h as f32) / 2.0;
        let w = 200.0;
        let h = 44.0;
        crate::setup::Rect { x: board_cx - w / 2.0, y: board_cy + 40.0, w, h }
    }

    // --- конец игры ---

    pub(crate) fn game_over_button_rect(&self) -> crate::setup::Rect {
        let w = 160.0;
        let h = 44.0;
        let first_button_top = self.buttons.first().map(|b| b.cy - b.radius).unwrap_or(200.0);
        let cx = (crate::layout::LEFT_PANEL as f32) / 2.0;
        let y = first_button_top - h - 30.0;
        crate::setup::Rect { x: cx - w / 2.0, y, w, h }
    }

    pub(crate) fn game_over_button_at(&self, px: f32, py: f32) -> bool {
        let visible = self.state.as_ref().map_or(false, |st| st.is_over()) || self.disconnect_overlay.is_some();
        if !visible { return false; }
        self.game_over_button_rect().contains(px, py)
    }

    // --- ожидание соперника ---

    pub(crate) fn waiting_cancel_button_rect(&self) -> crate::setup::Rect {
        let board_cx = self.layout.left() + (self.layout.board_w as f32) / 2.0;
        let board_cy = self.layout.top() + (self.layout.board_h as f32) / 2.0;
        let w = 200.0;
        let h = 44.0;
        crate::setup::Rect { x: board_cx - w / 2.0, y: board_cy + 70.0, w, h }
    }

    pub(crate) fn waiting_cancel_button_at(&self, px: f32, py: f32) -> bool {
        if !self.waiting_for_opponent { return false; }
        self.waiting_cancel_button_rect().contains(px, py)
    }

    pub(crate) fn return_to_setup(&mut self) {
        info!("[ui] возврат в меню");
        if let Some(net) = self.net.take() {
            let _ = net.cmd_tx.send(ClientCommand::Shutdown);
            drop(net);
        }
        self.pending_move_draw = None;
        self.waiting_for_state = false;
        self.waiting_for_state_since = None;
        self.draw_request_from_partner = None;
        self.draw_request_deadline = None;
        self.draw_offer_2v2 = None;
        self.resign_request_from_partner = None;
        self.resign_request_deadline = None;
        self.my_hidden_royals.clear();
        self.history.clear();
        self.review_index = None;
        self.last_move_squares.clear();
        self.my_seat = None;
        self.partner_name = None;
        self.opponents = None;
        self.setup.lobby = crate::setup::LobbyView::Idle;
        self.setup.focused_field = crate::setup::FocusField::None;
        self.state = None;
        self.my_color = None;
        self.opponent_name = None;
        self.game_id = None;
        self.connected = false;
        self.waiting_for_opponent = false;
        self.board_flipped = false;
        self.selected = None;
        self.legal_moves.clear();
        self.legal_castles.clear();
        self.capture_moves.clear();
        self.selected_reserve = None;
        self.drop_moves.clear();
        self.dragging = false;
        self.drag_from = None;
        self.dragging_reserve = false;
        self.drag_from_reserve = None;
        self.dragging_hidden_royal = None;
        self.promotion_pending = None;
        self.draw_offer_from = None;
        self.draw_offer_deadline = None;
        self.clock_local = None;
        self.clock_synced_at = None;
        self.disconnect_overlay = None;
        self.transient_message = None;
        self.hovered_button = None;
        self.hovered_since = None;
        self.invite_target = None;
        self.invite_retry_at = None;
        #[cfg(debug_assertions)]
        {
            self.bot_enabled = false;
            self.bot_thinking_until = None;
        }
        self.phase = Phase::Setup;
        self.bg_key = None;
        self.rebuild_reserve_slots();
        self.rebuild_buttons();
        if let Some(w) = &self.window { w.request_redraw(); }
    }

    pub(crate) fn start_game(&mut self) {
        if self.net.is_some() {
            warn!("[ui] start_game: сеть уже запущена");
            return;
        }
        let is_team = self.setup.is_team();
        let mode = self.setup.mode_str();
        info!(
            "[ui] start_game: name={:?} mode={} variant={} time={} is_team={} partner={:?} opponent={:?}",
            self.setup.name, mode, self.setup.variant,
            self.setup.time_control, is_team,
            self.setup.partner_name, self.setup.opponent_name
        );

        self.history.clear();
        self.review_index = None;
        self.last_move_squares.clear();

        self.my_name = self.setup.name.trim().to_string();
        let hello = chess_protocol::Hello {
            version: Some("0.1".into()),
            name: self.my_name.clone(),
            variant: self.setup.variant.to_string(),
            time_control: self.setup.time_control.to_string(),
            mode: mode.to_string(),
            opponent_name: if self.setup.opponent_name.trim().is_empty() {
                None
            } else {
                Some(self.setup.opponent_name.trim().to_string())
            },
            partner_name: if self.setup.partner_name.trim().is_empty() {
                None
            } else {
                Some(self.setup.partner_name.trim().to_string())
            },
        };
        let (cmd_tx, ev_rx, join) = chess_client_net::spawn_client("127.0.0.1".into(), 8080, hello);
        self.net = Some(crate::app::NetHandle { cmd_tx, ev_rx, _join: join });
        self.connected = false;
        self.phase = Phase::Playing;
        self.clear_selection();
        self.clear_reserve_selection();
        self.bg_key = None;
        self.waiting_for_opponent = true;
        self.setup.lobby = crate::setup::LobbyView::Idle;

        if mode == "team_2v2_pick" {
            let target = self.setup.partner_name.trim().to_string();
            if !target.is_empty() {
                if let Some(net) = self.net.as_ref() {
                    let _ = net.cmd_tx.send(ClientCommand::InvitePartner {
                        target_name: target.clone(),
                    });
                }
                info!("[ui] приглашение союзнику {:?} отправлено", target);
                self.invite_target = Some(target.clone());
                self.invite_retry_at = Some(Instant::now() + Duration::from_secs(2));
                self.transient_message = Some((
                    format!("приглашение отправлено {target}"),
                        Instant::now() + TRANSIENT_MSG_LIFETIME,
                ));
            }
        } else {
            self.invite_target = None;
            self.invite_retry_at = None;
        }

        #[cfg(debug_assertions)]
        {
            self.bot_enabled = self.setup.role == crate::setup::Role::Bot;
            self.bot_thinking_until = None;
        }

        if let Some(w) = &self.window { w.request_redraw(); }
    }

    #[cfg(debug_assertions)]
    pub(crate) fn try_bot_move(&mut self) {
        // Фикс 2: ждём State от сервера после отправленного хода.
        if self.waiting_for_state {
            return;
        }

        let (my, current_player, is_over, mode) = match (self.my_color, self.state.as_ref()) {
            (Some(c), Some(st)) => (c, st.current_player, st.is_over(), st.mode),
            _ => return,
        };
        if is_over || current_player != my {
            return;
        }

        // ============================================================
        // 2×2: правила очерёдности и обязательного возврата Royal.
        // ============================================================
        if mode == chess_logic::GameMode::Team2v2 {
            if let (Some(st), Some(my_seat)) = (self.state.as_ref(), self.my_seat) {
                let team = match my {
                    chess_logic::Color::White => &st.white_team,
                    chess_logic::Color::Black => &st.black_team,
                };
                let me = team.player(my_seat);

                if team.last_seat_moved == Some(my_seat) {
                    return;
                }
                if !me.alive {
                    return;
                }
                if let Some(pt) = me.must_return_royal() {
                    let valid = st.legal_royal_return_squares(my);
                    if let Some(&to) = valid.first() {
                        if let Some(net) = self.net.as_ref() {
                            let _ = net.cmd_tx.send(ClientCommand::ReturnRoyal {
                                royal_pt: pt,
                                to,
                            });
                            self.waiting_for_state = true;
                            self.waiting_for_state_since = Some(Instant::now());
                        }
                    }
                    self.bot_thinking_until = None;
                    return;
                }
                if me.must_skip_turn() {
                    self.bot_thinking_until = None;
                    return;
                }

                // Seat::Second ждёт, пока Seat::First сходит в этом окне.
                if my_seat == chess_logic::Seat::Second {
                    if team.last_seat_moved != Some(chess_logic::Seat::First) {
                        let first = team.player(chess_logic::Seat::First);
                        let first_active = first.alive
                        && !first.must_skip_turn()
                        && first.must_return_royal().is_none();
                        if first_active {
                            self.bot_thinking_until = None;
                            return;
                        }
                    }
                }
            }
        }

        // Задержка «раздумья».
        if let Some(until) = self.bot_thinking_until {
            if Instant::now() < until {
                return;
            }
        } else {
            self.bot_thinking_until = Some(Instant::now() + crate::app::BOT_THINK_DELAY);
            return;
        }

        // ============================================================
        // Сбор возможных ходов.
        // ============================================================
        let moves = {
            let st = match self.state.as_ref() {
                Some(s) => s,
                None => return,
            };
            let mut moves: Vec<BotMove> = Vec::new();

            for (sq, piece) in st.board.iter_pieces() {
                if piece.color != my {
                    continue;
                }
                if mode == chess_logic::GameMode::Team2v2 {
                    if let Some(seat) = self.my_seat {
                        if st.board.get_owner(sq) != Some(seat) {
                            continue;
                        }
                    }
                }
                for to in st.legal_moves(sq) {
                    moves.push(BotMove::Board { from: sq, to, is_castle: false });
                }
                for to in st.legal_castles(sq) {
                    moves.push(BotMove::Board { from: sq, to, is_castle: true });
                }
            }

            if mode != chess_logic::GameMode::Team2v2 {
                if let Some(list) = st.reserve.get(&my) {
                    for &pt in list {
                        for to in st.legal_drops(pt) {
                            moves.push(BotMove::Drop { piece: pt, to });
                        }
                    }
                }
            }
            moves
        };

        if moves.is_empty() {
            info!("[bot] нет ходов");
            return;
        }

        let chosen = moves[moves.len().wrapping_sub(1) - (rand::random::<usize>() % moves.len())];
        self.bot_thinking_until = None;

        match chosen {
            BotMove::Board { from, to, is_castle } => {
                let needs_promotion = self.state.as_ref()
                .and_then(|st| st.board.get(from))
                .map_or(false, |p| {
                    p.piece_type == PieceType::Pawn
                    && to.row == my.pawn_promotion_row(self.layout.rows() as i8)
                });

                self.send_move(from, to, is_castle);
                if needs_promotion {
                    if let Some(promo) = self.promotion_pending.take() {
                        self.apply_promotion_choice(promo, PieceType::Queen);
                    }
                }
            }
            BotMove::Drop { piece, to } => {
                self.send_drop(my, piece, to);
            }
        }
    }

    pub(crate) fn update_review_last_move(&mut self) {
        let idx = match self.review_index {
            Some(i) => i,
            None => {
                if self.history.is_empty() { self.last_move_squares.clear(); return; }
                let last = match self.history.last() {
                    Some(s) => s,
                    None => { self.last_move_squares.clear(); return; }
                };
                let state_board = match self.state.as_ref() {
                    Some(st) => &st.board,
                    None => { self.last_move_squares.clear(); return; }
                };
                self.last_move_squares = compute_last_move(&last.board, state_board);
                return;
            }
        };
        if idx == 0 { self.last_move_squares.clear(); return; }
        let prev = match self.history.get(idx - 1) {
            Some(s) => s,
            None => { self.last_move_squares.clear(); return; }
        };
        let cur = match self.history.get(idx) {
            Some(s) => s,
            None => { self.last_move_squares.clear(); return; }
        };
        self.last_move_squares = compute_last_move(&prev.board, &cur.board);
    }

    pub(crate) fn poll_net_events(&mut self) -> bool {
        let mut got_any = false;
        let mut events: Vec<ClientEvent> = Vec::new();
        if let Some(net) = self.net.as_ref() {
            loop {
                match net.ev_rx.try_recv() {
                    Ok(ev) => events.push(ev),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        events.push(ClientEvent::ConnectionLost { reason: "сетевой поток завершился".into() });
                        break;
                    }
                }
            }
        }
        if events.is_empty() { return false; }

        for ev in events {
            got_any = true;
            match ev {
                ClientEvent::Connected => { self.connected = true; }
                ClientEvent::Welcome { color, game_id, opponent_name, seat, partner_name, opponents } => {
                    self.my_color = Some(color);
                    self.my_seat = seat;
                    self.game_id = Some(game_id.clone());
                    self.opponent_name = opponent_name.clone();
                    self.partner_name = partner_name.clone();
                    self.opponents = opponents.clone();
                    self.board_flipped = color == Color::Black;
                    self.waiting_for_opponent = false;
                    self.rebuild_reserve_slots();
                    self.bg_key = None;
                }
                ClientEvent::State(state) => {
                    // Фикс 2: State пришёл — разблокируем бота.
                    self.consecutive_server_errors = 0;
                    self.waiting_for_state = false;
                    self.waiting_for_state_since = None;
                    // Снимаем «виртуальный сдвиг»: State уже содержит
                    // актуальное положение фигур.
                    self.pending_move_draw = None;

                    // Подсветка последнего хода.
                    if let Some(prev) = self.state.as_ref() {
                        let squares = compute_last_move(&prev.board, &state.board);
                        if !squares.is_empty() {
                            self.last_move_squares = squares;
                        }
                    }

                    let _is_2v2 = state.mode == chess_logic::GameMode::Team2v2;

                    if let Some(clock) = state.clock.as_ref() {
                        self.clock_local = Some((clock.white, clock.black));
                        self.clock_synced_at = Some(Instant::now());
                    } else {
                        self.clock_local = None;
                        self.clock_synced_at = None;
                    }

                    let position_changed = match self.state.as_ref() {
                        Some(prev) => {
                            prev.current_player != state.current_player || prev.board != state.board
                        }
                        None => true,
                    };

                    let mode = state.mode;

                    self.state = Some(state);

                    if mode != self.layout.mode {
                        self.layout = Layout::from_window(self.width, self.height, mode);
                        self.rebuild_reserve_slots();
                        self.bg_key = None;
                    }

                    if self.state.as_ref().map_or(false, |st| st.is_over()) {
                        self.draw_offer_from = None;
                        self.draw_offer_deadline = None;
                        self.draw_offer_2v2 = None;
                        self.draw_request_from_partner = None;
                        self.draw_request_deadline = None;
                        self.resign_request_from_partner = None;
                        self.resign_request_deadline = None;
                        self.bg_key = None;
                        #[cfg(debug_assertions)]
                        {
                            self.bot_enabled = false;
                            self.bot_thinking_until = None;
                        }
                    }

                    self.my_hidden_royals.clear();
                    if let (Some(st), Some(my_color), Some(my_seat)) =
                        (self.state.as_ref(), self.my_color, self.my_seat)
                        {
                            let team = match my_color {
                                Color::White => &st.white_team,
                                Color::Black => &st.black_team,
                            };
                            let p = team.player(my_seat);
                            if p.is_in_holding(PieceType::Prince) && p.get_cd(PieceType::Prince) == 0 {
                                self.my_hidden_royals.push((PieceType::Prince, my_seat));
                            }
                            if p.is_in_holding(PieceType::Princess) && p.get_cd(PieceType::Princess) == 0 {
                                self.my_hidden_royals.push((PieceType::Princess, my_seat));
                            }
                        }

                        // Фикс: возобновляем бота, если сейчас его ход.
                        #[cfg(debug_assertions)]
                        {
                            if self.setup.role == crate::setup::Role::Bot
                                && !self.state.as_ref().map_or(true, |st| st.is_over())
                                && self.state.as_ref().map(|st| st.current_player)
                                == self.my_color
                                {
                                    self.bot_enabled = true;
                                    self.bot_thinking_until = None;
                                }
                        }

                        if position_changed {
                            self.clear_selection();
                        } else if let Some(sel) = self.selected {
                            if let Some(st) = self.state.as_ref() {
                                let lm = st.legal_moves(sel);
                                let lc = st.legal_castles(sel);
                                let mut cap = HashSet::new();
                                for m in &lm {
                                    if st.board.get(*m).is_some() {
                                        cap.insert(*m);
                                    }
                                }
                                self.legal_moves = lm;
                                self.legal_castles = lc;
                                self.capture_moves = cap;
                            }
                        }
                        if let Some((_, pt)) = self.selected_reserve {
                            if let Some(st) = self.state.as_ref() {
                                self.drop_moves = st.legal_drops(pt);
                            }
                        }
                        self.bg_key = None;
                }

                ClientEvent::DrawOffer { by, by_names } => {
                    if Some(by) != self.my_color {
                        if let Some(names) = by_names {
                            self.draw_offer_2v2 = Some(crate::app::DrawOffer2v2 {
                                by_color: by,
                                by_names: names,
                                deadline: Instant::now() + Duration::from_secs(10),
                                                       accepted_by_me: false,
                            });
                        } else {
                            self.draw_offer_from = Some(by);
                            self.draw_offer_deadline = Some(Instant::now() + Duration::from_secs(10));
                        }
                        self.bg_key = None;
                        #[cfg(debug_assertions)]
                        if self.bot_enabled {
                            let _ = self.net.as_ref().map(|n| n.cmd_tx.send(ClientCommand::DeclineDraw));
                            self.draw_offer_from = None;
                            self.draw_offer_deadline = None;
                        }
                    }
                }
                ClientEvent::DrawDeclined => {
                    self.draw_offer_2v2 = None;
                    self.draw_offer_from = None;
                    self.draw_offer_deadline = None;
                    self.transient_message = Some(("ничья отклонена".into(), Instant::now() + TRANSIENT_MSG_LIFETIME));
                    self.bg_key = None;
                }
                ClientEvent::GameOver { reason } => {
                    info!("[ui] GameOver: {:?}, history={}", reason, self.history.len());
                    info!("[ui] сеть: GameOver reason={:?}", reason);

                    // Фикс 2: игра закончена — снимаем блокировку.
                    self.waiting_for_state = false;
                    self.waiting_for_state_since = None;
                    // State больше не придёт — снимаем «виртуальный сдвиг».
                    self.pending_move_draw = None;

                    self.draw_offer_from = None;
                    self.draw_offer_deadline = None;
                    self.promotion_pending = None;

                    #[cfg(debug_assertions)]
                    {
                        self.bot_enabled = false;
                        self.bot_thinking_until = None;
                    }
                    self.bg_key = None;
                }
                ClientEvent::OpponentDisconnected => {
                    self.waiting_for_opponent = false;
                    // Игра завершена — снимаем «виртуальный сдвиг»,
                    // чтобы фигуры отрисовались по последнему известному state.
                    self.pending_move_draw = None;
                    self.disconnect_overlay = Some(DisconnectOverlay {
                        title: "Соперник отключился".to_string(),
                                                   subtitle: "Игра завершена".to_string(),
                                                   kind: DisconnectKind::OpponentLeft,
                    });
                    self.bg_key = None;
                }
                ClientEvent::ConnectionLost { reason } => {
                    info!("[ui] ConnectionLost: {}", reason);
                    self.connected = false;
                    self.waiting_for_opponent = false;

                    // Фикс 2: связь потеряна — снимаем блокировку.
                    self.waiting_for_state = false;
                    self.waiting_for_state_since = None;
                    // State уже не придёт — снимаем «виртуальный сдвиг».
                    self.pending_move_draw = None;

                    #[cfg(debug_assertions)]
                    {
                        self.bot_enabled = false;
                        self.bot_thinking_until = None;
                    }

                    if self.state.as_ref().map_or(false, |st| st.is_over()) {
                        info!("[ui] ConnectionLost после GameOver — игнорируем");
                    } else {
                        self.disconnect_overlay = Some(DisconnectOverlay {
                            title: "Связь потеряна".to_string(),
                                                       subtitle: reason,
                                                       kind: DisconnectKind::ConnectionLost,
                        });
                    }
                    self.bg_key = None;
                }
                ClientEvent::ServerError { message, code: _ } => {
                    self.consecutive_server_errors += 1;
                    if self.consecutive_server_errors >= 3 {
                        warn!("[ui] 3+ ServerError подряд — выключаем бота");
                        #[cfg(debug_assertions)]
                        {
                            self.bot_enabled = false;
                            self.bot_thinking_until = None;
                        }
                    }
                    // НЕ сбрасываем waiting_for_state и НЕ перезаписываем since,
                    // если уже ждём State — иначе таймаут будет откладываться
                    // бесконечно, пока сервер шлёт ServerError.
                    if !self.waiting_for_state {
                        self.waiting_for_state = true;
                        self.waiting_for_state_since = Some(Instant::now());
                    }
                    self.pending_move_draw = None;

                    self.transient_message = Some((message.clone(), Instant::now() + TRANSIENT_MSG_LIFETIME));

                    if let Some(net) = self.net.as_ref() {
                        let _ = net.cmd_tx.send(ClientCommand::RequestState);
                    }
                    self.bg_key = None;
                }
                ClientEvent::InviteSent { .. } => {}
                ClientEvent::InviteReceived { from_name, variant, time_control } => {
                    if self.phase == Phase::Playing
                        && self.setup.mode_str() == "team_2v2_pick"
                        && !self.setup.partner_name.trim().is_empty()
                        && self.setup.partner_name.trim().eq_ignore_ascii_case(&from_name)
                        {
                            info!("[ui] встречное приглашение от {from_name} — принимаем автоматически");
                            if let Some(net) = self.net.as_ref() {
                                let _ = net.cmd_tx.send(ClientCommand::AcceptInvite {
                                    from_name: from_name.clone(),
                                });
                            }
                            self.transient_message = Some((
                                format!("принимаем приглашение от {from_name}"),
                                    Instant::now() + TRANSIENT_MSG_LIFETIME,
                            ));
                            self.bg_key = None;
                        } else {
                            self.setup.lobby = crate::setup::LobbyView::InviteReceived {
                                from_name,
                                variant,
                                time_control,
                            };
                            self.bg_key = None;
                        }
                }
                ClientEvent::InviteCancelled { from_name } => {
                    if let crate::setup::LobbyView::InviteReceived { from_name: ref cur, .. } = self.setup.lobby {
                        if *cur == from_name { self.setup.lobby = crate::setup::LobbyView::Idle; }
                    }
                    self.bg_key = None;
                }
                ClientEvent::InviteDeclined { by_name } => {
                    self.setup.lobby = crate::setup::LobbyView::Idle;
                    self.transient_message = Some((format!("{by_name} отклонил приглашение"), Instant::now() + TRANSIENT_MSG_LIFETIME));
                    self.bg_key = None;
                }
                ClientEvent::InviteError { code, message } => {
                    let fatal = matches!(code.as_str(), "wrong_mode" | "self" | "mismatch");
                    if fatal {
                        self.invite_target = None;
                        self.invite_retry_at = None;
                    }
                    self.setup.lobby = crate::setup::LobbyView::Idle;
                    self.transient_message = Some((format!("приглашение: {message}"), Instant::now() + TRANSIENT_MSG_LIFETIME));
                    self.bg_key = None;
                }
                ClientEvent::TeamFormed { partner_name, you_are } => {
                    self.my_seat = Some(you_are);
                    self.partner_name = Some(partner_name.clone());
                    self.setup.lobby = crate::setup::LobbyView::InPair { partner_name };
                    self.invite_target = None;
                    self.invite_retry_at = None;
                    self.bg_key = None;
                }
                ClientEvent::PartnerLeft { partner_name } => {
                    self.partner_name = None;
                    self.setup.lobby = crate::setup::LobbyView::Idle;
                    self.transient_message = Some((format!("{partner_name} покинул команду"), Instant::now() + TRANSIENT_MSG_LIFETIME));
                    self.bg_key = None;
                }
                ClientEvent::DrawRequestToPartner { from_name } => {
                    self.draw_request_from_partner = Some(from_name);
                    self.draw_request_deadline = Some(Instant::now() + Duration::from_secs(10));
                    self.bg_key = None;
                }
                ClientEvent::DrawRequestDeclined => {
                    self.transient_message = Some(("партнёр отказался предлагать ничью".into(), Instant::now() + TRANSIENT_MSG_LIFETIME));
                    self.bg_key = None;
                }
                ClientEvent::DrawOfferAccepted { by_color, by_seat } => {
                    if Some(by_color) == self.my_color && Some(by_seat) == self.my_seat {
                        if let Some(ref mut offer) = self.draw_offer_2v2 {
                            offer.accepted_by_me = true;
                        }
                    }
                    self.transient_message = Some((format!("{:?} {:?} принял ничью", by_color, by_seat), Instant::now() + TRANSIENT_MSG_LIFETIME));
                    self.bg_key = None;
                }
                ClientEvent::ResignRequestToPartner { from_name } => {
                    self.resign_request_from_partner = Some(from_name);
                    self.resign_request_deadline = Some(Instant::now() + Duration::from_secs(10));
                    self.bg_key = None;
                    if let Some(w) = &self.window { w.request_redraw(); }
                }
                ClientEvent::ResignRequestDeclined => {
                    self.transient_message = Some(("партнёр отказался сдаваться".into(), Instant::now() + TRANSIENT_MSG_LIFETIME));
                    self.bg_key = None;
                }
                ClientEvent::TeamResigned { by_color } => {
                    self.transient_message = Some((format!("{:?} сдались", by_color), Instant::now() + TRANSIENT_MSG_LIFETIME));
                    self.bg_key = None;
                }
                ClientEvent::History { snapshots } => {
                    info!("[ui] History: {} snapshots", snapshots.len());
                    self.history = snapshots;
                    self.review_index = None;
                    self.update_review_last_move();
                    self.bg_key = None;
                    if let Some(w) = &self.window { w.request_redraw(); }
                }
            }
        }

        got_any
    }
}

// ============================================================
// Свободные функции
// ============================================================

fn compute_last_move(
    prev: &chess_logic::Board,
    next: &chess_logic::Board,
) -> Vec<chess_logic::Square> {
    use chess_logic::Square;
    let mut prev_map: HashMap<(i8, i8), (chess_logic::Color, chess_logic::PieceType)> = HashMap::new();
    for (sq, p) in prev.iter_pieces() {
        prev_map.insert((sq.col, sq.row), (p.color, p.piece_type));
    }
    let mut next_map: HashMap<(i8, i8), (chess_logic::Color, chess_logic::PieceType)> = HashMap::new();
    for (sq, p) in next.iter_pieces() {
        next_map.insert((sq.col, sq.row), (p.color, p.piece_type));
    }
    let mut squares: Vec<Square> = Vec::new();
    for (&(c, r), val) in prev_map.iter() {
        match next_map.get(&(c, r)) {
            Some(v) if v == val => {}
            _ => squares.push(Square::new(c, r)),
        }
    }
    for (&(c, r), val) in next_map.iter() {
        match prev_map.get(&(c, r)) {
            Some(v) if v == val => {}
            _ => squares.push(Square::new(c, r)),
        }
    }
    squares.sort_by_key(|s| (s.col, s.row));
    squares.dedup_by_key(|s| (s.col, s.row));
    squares
}
