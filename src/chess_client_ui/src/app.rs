//! Application: структуры, конструктор, resize, геометрия, курсор, часы.

#[path = "app_handlers.rs"]
pub(crate) mod handlers;
#[path = "app_render_board.rs"]
pub(crate) mod render_board;
#[path = "app_render_buttons.rs"]
pub(crate) mod render_buttons;
#[path = "app_render_overlays.rs"]
pub(crate) mod render_overlays;

use crate::assets::PieceAtlas;
use crate::layout::{Layout, RIGHT_PANEL};
use crate::setup::{SetupLayout, SetupState};
use crate::text::TextRenderer;
use chess_client_net::{spawn_client, ClientCommand, ClientEvent};
use chess_logic::{Color, GameMode, GameState, Piece, PieceType, Seat, Square};
use chess_protocol::Hello;
use softbuffer::{Context, Surface};
use std::collections::HashSet;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;
use tokio::sync::mpsc as tokio_mpsc;
use tracing::{error, info, warn};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

// --- константы ---


pub(crate) const LIGHT: (u8, u8, u8) = (240, 217, 181);
pub(crate) const DARK: (u8, u8, u8) = (181, 136, 99);
pub(crate) const BG: (u8, u8, u8) = (30, 30, 30);
pub(crate) const BOARD_BORDER: (u8, u8, u8) = (120, 120, 140);

pub(crate) const SETUP_BG: (u8, u8, u8) = (30, 30, 40);
pub(crate) const SETUP_FIELD_BG: (u8, u8, u8) = (45, 45, 60);
pub(crate) const SETUP_FIELD_BORDER: (u8, u8, u8) = (100, 100, 130);
pub(crate) const SETUP_FIELD_BORDER_FOCUS: (u8, u8, u8) = (120, 200, 255);
pub(crate) const SETUP_OPTION_BG: (u8, u8, u8) = (45, 45, 60);
pub(crate) const SETUP_OPTION_BG_SELECTED: (u8, u8, u8) = (60, 100, 160);
pub(crate) const SETUP_OPTION_BORDER: (u8, u8, u8) = (100, 100, 130);
pub(crate) const SETUP_OPTION_BORDER_SELECTED: (u8, u8, u8) = (140, 180, 240);
pub(crate) const SETUP_OPTION_DISABLED: (u8, u8, u8) = (70, 70, 80);
pub(crate) const SETUP_PLAY_ACTIVE: (u8, u8, u8) = (60, 140, 60);
pub(crate) const SETUP_PLAY_INACTIVE: (u8, u8, u8) = (60, 60, 70);
pub(crate) const SETUP_TEXT: (u8, u8, u8) = (230, 230, 240);
pub(crate) const SETUP_TEXT_DIM: (u8, u8, u8) = (140, 140, 150);
pub(crate) const SETUP_TITLE: (u8, u8, u8) = (255, 220, 150);

pub(crate) const NAME_NORMAL: (u8, u8, u8) = (220, 220, 240);

pub(crate) const HL_SELECTED: (u8, u8, u8) = (0, 220, 0);
pub(crate) const HL_MOVE_DOT: (u8, u8, u8) = (30, 160, 30);
pub(crate) const HL_CHECK: (u8, u8, u8) = (220, 30, 30);

pub(crate) const NET_POLL_INTERVAL: Duration = Duration::from_millis(30);
pub(crate) const DRAG_POLL_INTERVAL: Duration = Duration::from_millis(8);
pub(crate) const CLOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);
pub(crate) const TOOLTIP_DELAY: Duration = Duration::from_millis(700);
pub(crate) const TRANSIENT_MSG_LIFETIME: Duration = Duration::from_secs(3);

pub(crate) const CLOCK_RED_THRESHOLD_SECS: u64 = 30;

#[cfg(debug_assertions)]
pub(crate) const BOT_THINK_DELAY: Duration = Duration::from_millis(100);

// --- структуры ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReviewAction {
    Start,
    Prev,
    Next,
    End,
}

#[derive(Clone, Debug)]
pub(crate) struct DrawOffer2v2 {
    pub by_color: Color,
    pub by_names: [String; 2],
    pub deadline: Instant,
    pub accepted_by_me: bool,
}

pub(crate) struct NetHandle {
    pub cmd_tx: tokio_mpsc::UnboundedSender<ClientCommand>,
    pub ev_rx: std::sync::mpsc::Receiver<ClientEvent>,
    pub _join: std::thread::JoinHandle<()>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Setup,
    Playing,
}


#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct BgKey {
    pub(crate) board_hash: u64,
    pub(crate) selected: Option<(i8, i8)>,
    pub(crate) current_player: Color,
    pub(crate) is_over: bool,
    pub(crate) my_color: Option<Color>,
    pub(crate) board_flipped: bool,
    pub(crate) variant_tag: u8,
    pub(crate) dragging: bool,
    pub(crate) drag_from: Option<(i8, i8)>,
    pub(crate) dragging_reserve: bool,
    pub(crate) drag_from_reserve: Option<(Color, PieceType)>,
    pub(crate) draw_offer_from: Option<Color>,
    pub(crate) hovered_button: Option<ButtonKind>,
    pub(crate) promotion_open: bool,
    pub(crate) disconnect_open: bool,
    pub(crate) waiting: bool,
    pub(crate) last_move_from: Option<(i8, i8)>,
    pub(crate) last_move_to: Option<(i8, i8)>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReserveSlot {
    pub(crate) color: Color,
    pub(crate) piece_type: PieceType,
    pub(crate) cx: f32,
    pub(crate) cy: f32,
    pub(crate) size: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ButtonKind {
    Flip,
    Resign,
    Draw,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Button {
    pub kind: ButtonKind,
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PromotionPending {
    pub from: Square,
    pub to: Square,
    pub color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisconnectKind {
    OpponentLeft,
    ConnectionLost,
}


#[derive(Clone, Debug)]
pub(crate) struct DisconnectOverlay {
    pub title: String,
    pub subtitle: String,
    pub kind: DisconnectKind,
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum BotMove {
    Board {
        from: Square,
        to: Square,
        is_castle: bool,
    },
    Drop {
        piece: PieceType,
        to: Square,
    },
}



pub(crate) struct RoyalSlot {
    pub color: Color,
    pub seat: Seat,
    pub pt: PieceType,
    pub cx: f32,
    pub cy: f32,
    pub size: f32,
}



pub struct App {
    pub(crate) invite_retry_at: Option<Instant>,
    pub(crate) invite_target: Option<String>,

    pub(crate) history: Vec<chess_logic::state::HistorySnapshot>,
    pub(crate) review_index: Option<usize>,

    pub(crate) review_buttons: Vec<(ReviewAction, crate::setup::Rect)>,

    pub(crate) resign_request_from_partner: Option<String>,
    pub(crate) resign_request_deadline: Option<Instant>,

    pub(crate) dragging_hidden_royal: Option<(PieceType, Seat)>,
    pub(crate) royal_slots: Vec<RoyalSlot>,

    pub(crate) window: Option<Arc<Window>>,
    pub(crate) context: Option<Context<Arc<Window>>>,
    pub(crate) surface: Option<Surface<Arc<Window>, Arc<Window>>>,

    pub(crate) pixmap: Pixmap,
    pub(crate) bg_pixmap: Pixmap,
    pub(crate) bg_key: Option<BgKey>,

    pub(crate) width: u32,
    pub(crate) height: u32,

    pub(crate) layout: Layout,
    pub(crate) atlas: PieceAtlas,

    pub(crate) text: TextRenderer,

    pub(crate) phase: Phase,
    pub(crate) setup: SetupState,
    pub(crate) setup_layout: SetupLayout,

    pub(crate) pointer_pos: (f64, f64),

    pub(crate) net: Option<NetHandle>,
    pub(crate) my_color: Option<Color>,
    pub(crate) opponent_name: Option<String>,
    pub(crate) game_id: Option<String>,
    pub(crate) state: Option<GameState>,
    pub(crate) connected: bool,

    // --- 2x2 ---
    pub(crate) my_seat: Option<Seat>,
    pub(crate) partner_name: Option<String>,
    pub(crate) opponents: Option<[String; 2]>,
    pub(crate) my_hidden_royals: Vec<(PieceType, Seat)>,

    // --- 2x2: ничья ---
    pub(crate) draw_request_from_partner: Option<String>,
    pub(crate) draw_request_deadline: Option<Instant>,
    pub(crate) draw_offer_2v2: Option<DrawOffer2v2>,

    pub(crate) waiting_for_opponent: bool,

    pub(crate) board_flipped: bool,
    pub(crate) my_name: String,

    pub(crate) selected: Option<Square>,
    pub(crate) legal_moves: Vec<Square>,
    pub(crate) legal_castles: Vec<Square>,
    pub(crate) capture_moves: HashSet<Square>,

    pub(crate) mouse_pressed: bool,
    pub(crate) mouse_press_x: f32,
    pub(crate) mouse_press_y: f32,
    pub(crate) dragging: bool,
    pub(crate) drag_from: Option<Square>,
    pub(crate) drag_pixel_x: f32,
    pub(crate) drag_pixel_y: f32,
    pub(crate) drag_threshold: f32,

    pub(crate) selected_reserve: Option<(Color, PieceType)>,
    pub(crate) drop_moves: Vec<Square>,
    pub(crate) dragging_reserve: bool,
    pub(crate) drag_from_reserve: Option<(Color, PieceType)>,
    pub(crate) reserve_slots: Vec<ReserveSlot>,

    pub(crate) buttons: Vec<Button>,
    pub(crate) hovered_button: Option<ButtonKind>,
    pub(crate) hovered_since: Option<Instant>,
    pub(crate) tooltip_shown: bool,

    pub(crate) draw_offer_from: Option<Color>,
    pub(crate) draw_offer_deadline: Option<Instant>,

    pub(crate) promotion_pending: Option<PromotionPending>,

    pub(crate) clock_local: Option<(Duration, Duration)>,
    pub(crate) clock_synced_at: Option<Instant>,

    pub(crate) disconnect_overlay: Option<DisconnectOverlay>,
    pub(crate) transient_message: Option<(String, Instant)>,
    //pub(crate) my_hidden_royals: Vec<(PieceType, Seat)>,

    pub(crate) last_move_from: Option<Square>,
    pub(crate) last_move_to: Option<Square>,

    pub(crate) last_seen_seat_moved: Option<Seat>,

    // последний ход (штриховка)
    pub(crate) last_move_squares: Vec<chess_logic::Square>,

    pub(crate) waiting_for_state: bool,
    pub(crate) waiting_for_state_since: Option<Instant>,
    pub(crate) hidden_source: Option<Square>,
    pub(crate) pending_move_draw: Option<(chess_logic::Square, chess_logic::Square)>,
    pub(crate) consecutive_server_errors: u32,

    #[cfg(debug_assertions)]
    pub(crate) bot_enabled: bool,
    #[cfg(debug_assertions)]
    pub(crate) bot_thinking_until: Option<Instant>,
}

impl App {
    pub fn new() -> Self {
        let pixmap = Pixmap::new(800, 600)
        .expect("Pixmap::new(800, 600) должен работать");
        let bg_pixmap = Pixmap::new(800, 600)
        .expect("Pixmap::new(800, 600) должен работать");
        let layout = Layout::from_window(800, 600, chess_logic::GameMode::Duel);
        let atlas = PieceAtlas::build(layout.cell);

        let font_bytes: &[u8] =
        include_bytes!("../assets/fonts/DejaVuSansMono.ttf");
        let text = TextRenderer::from_bytes(font_bytes)
        .expect("не удалось загрузить встроенный шрифт");

        info!("[ui] атлас: {} фигур, cell={}", atlas.len(), layout.cell);

        let setup = SetupState::new();
        let setup_layout = SetupLayout::from_window(800, 600);

        let mut app = Self {
            review_buttons: Vec::new(),
            history: Vec::new(),
            invite_retry_at: None,
            invite_target: None,
            review_index: None,
            resign_request_from_partner: None,
            resign_request_deadline: None,
            dragging_hidden_royal: None,
            my_hidden_royals: Vec::new(),
            royal_slots: Vec::new(),
            window: None,
            context: None,
            surface: None,
            pixmap,
            bg_pixmap,
            bg_key: None,
            width: 800,
            height: 600,
            layout,
            atlas,
            text,
            phase: Phase::Setup,
            setup,
            setup_layout,
            pointer_pos: (0.0, 0.0),
            net: None,
            my_color: None,
            opponent_name: None,
            game_id: None,
            state: None,
            connected: false,
            my_seat: None,
            partner_name: None,
            opponents: None,
            //my_hidden_royals: Vec::new(),
            draw_request_from_partner: None,
            draw_request_deadline: None,
            draw_offer_2v2: None,
            waiting_for_opponent: false,
            board_flipped: false,
            my_name: String::new(),
            selected: None,
            legal_moves: Vec::new(),
            legal_castles: Vec::new(),
            capture_moves: HashSet::new(),
            mouse_pressed: false,
            mouse_press_x: 0.0,
            mouse_press_y: 0.0,
            dragging: false,
            drag_from: None,
            drag_pixel_x: 0.0,
            drag_pixel_y: 0.0,
            drag_threshold: 5.0,
            selected_reserve: None,
            drop_moves: Vec::new(),
            dragging_reserve: false,
            drag_from_reserve: None,
            reserve_slots: Vec::new(),
            buttons: Vec::new(),
            hovered_button: None,
            hovered_since: None,
            tooltip_shown: false,
            draw_offer_from: None,
            draw_offer_deadline: None,
            promotion_pending: None,
            clock_local: None,
            clock_synced_at: None,
            disconnect_overlay: None,
            transient_message: None,
            #[cfg(debug_assertions)]
            bot_enabled: false,
            #[cfg(debug_assertions)]
            bot_thinking_until: None,
            last_move_from: None,
            last_move_to: None,
            last_seen_seat_moved: None,
            last_move_squares: Vec::new(),
            waiting_for_state: false,
            waiting_for_state_since: None,
            hidden_source:None,
            pending_move_draw: None,
            consecutive_server_errors: 0,
        };
        app.rebuild_reserve_slots();
        app.rebuild_buttons();
        app
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        let is_2v2 = self.state.as_ref().map_or(false, |st| st.mode == chess_logic::GameMode::Team2v2);
        let mode = self.state.as_ref().map_or(chess_logic::GameMode::Duel, |st| st.mode);
        self.layout = Layout::from_window(width, height, mode);
        self.setup_layout = SetupLayout::from_window(width, height);

        if self.atlas.cell != self.layout.cell {
            self.atlas = PieceAtlas::build(self.layout.cell);
        }

        self.rebuild_reserve_slots();
        self.rebuild_buttons();
        self.bg_key = None;

        if let Some(surface) = self.surface.as_mut() {
            let nz_w = match NonZeroU32::new(width) {
                Some(v) => v,
                None => return,
            };
            let nz_h = match NonZeroU32::new(height) {
                Some(v) => v,
                None => return,
            };
            if let Err(e) = surface.resize(nz_w, nz_h) {
                warn!("[ui] surface.resize: {e}");
                return;
            }
        }

        match Pixmap::new(width, height) {
            Some(p) => self.pixmap = p,
            None => warn!("[ui] не удалось создать pixmap {width}x{height}"),
        }
        match Pixmap::new(width, height) {
            Some(p) => self.bg_pixmap = p,
            None => warn!("[ui] не удалось создать bg_pixmap {width}x{height}"),
        }
    }

    pub(crate) fn rebuild_reserve_slots(&mut self) {
        self.reserve_slots.clear();

        // В 2х2 нет запасных полей
        if let Some(st) = self.state.as_ref() {
            if st.mode == chess_logic::GameMode::Team2v2 {
                return;
            }
        }

        let cell = self.layout.cell as f32;
        let slot_size = cell * 0.72;
        let gap = (cell / 20.0).max(4.0);
        let panel_x = self.layout.right() + crate::layout::MARGIN as f32;
        let panel_cx = panel_x + (RIGHT_PANEL as f32) / 2.0;

        let (top_color, bottom_color) = if self.board_flipped {
            (Color::White, Color::Black)
        } else {
            (Color::Black, Color::White)
        };

        let types_order = [PieceType::Cheetah, PieceType::Genie, PieceType::Pawn];

        let top_start_y = self.layout.top() + slot_size / 2.0;
        for (i, t) in types_order.iter().enumerate() {
            let cy = top_start_y + i as f32 * (slot_size + gap);
            self.reserve_slots.push(ReserveSlot {
                color: top_color,
                piece_type: *t,
                cx: panel_cx,
                cy,
                size: slot_size,
            });
        }

        let bottom_end_y = self.layout.top() + (self.layout.board_h as f32) - slot_size / 2.0;
        let bottom_start_y = bottom_end_y - (types_order.len() as f32 - 1.0) * (slot_size + gap);
        for (i, t) in types_order.iter().enumerate() {
            let cy = bottom_start_y + i as f32 * (slot_size + gap);
            self.reserve_slots.push(ReserveSlot {
                color: bottom_color,
                piece_type: *t,
                cx: panel_cx,
                cy,
                size: slot_size,
            });
        }

        // Слоты для Royal'ов в укрытии (только в 2х2).
        self.royal_slots.clear();
        if let Some(st) = self.state.as_ref() {
            if st.mode == GameMode::Team2v2 {
                let panel_x = self.layout.right() + crate::layout::MARGIN as f32;
                let slot_size = self.layout.cell as f32 * 0.8;
                let gap = 10.0;
                let mut y = self.layout.top() + slot_size / 2.0;

                // Белые Royal'ы
                for pt in [PieceType::Princess, PieceType::Prince] {
                    self.royal_slots.push(RoyalSlot {
                        color: Color::White,
                        seat: Seat::First, // Уточним при отрисовке
                        pt,
                        cx: panel_x + slot_size / 2.0,
                        cy: y,
                        size: slot_size,
                    });
                    y += slot_size + gap;
                }
                // ... аналогично для Черных
            }
        }


    }

    pub(crate) fn rebuild_buttons(&mut self) {
        self.buttons.clear();

        let panel_cx = (crate::layout::LEFT_PANEL as f32) / 2.0;
        let radius = 38.0;
        let gap = 16.0;

        // Сдачу показываем всегда (в 2×2 она через протокол с партнёром).
        let kinds = vec![ButtonKind::Flip, ButtonKind::Resign, ButtonKind::Draw];


        let total_h = kinds.len() as f32 * (2.0 * radius + gap) - gap;
        let center_y = (self.height as f32) / 2.0;
        let start_y = center_y - total_h / 2.0 + radius;

        for (i, kind) in kinds.iter().enumerate() {
            self.buttons.push(Button {
                kind: *kind,
                cx: panel_cx,
                cy: start_y + i as f32 * (2.0 * radius + gap),
                              radius,
            });
        }
    }

    pub(crate) fn button_at(&self, px: f32, py: f32) -> Option<Button> {
        for b in &self.buttons {
            let dx = px - b.cx;
            let dy = py - b.cy;
            if dx * dx + dy * dy <= b.radius * b.radius {
                return Some(*b);
            }
        }
        None
    }

    // --- геометрия ---

    // --- геометрия ---

    pub(crate) fn cell_origin(&self, col: u32, row: u32) -> (f32, f32) {
        if col >= self.layout.cols() || row >= self.layout.rows() {
            return (0.0, 0.0);
        }
        let (c, r) = if self.board_flipped {
            (self.layout.cols() - 1 - col, self.layout.rows() - 1 - row)
        } else {
            (col, row)
        };
        self.layout.cell_origin(c, r)
    }

    pub(crate) fn pixel_to_cell(&self, px: f32, py: f32) -> Option<Square> {
        let lx = self.layout.left();
        let ly = self.layout.top();
        let w = self.layout.board_w as f32;
        let h = self.layout.board_h as f32;

        if px < lx || py < ly || px >= lx + w || py >= ly + h {
            return None;
        }

        let cell = self.layout.cell as f32;
        let col_scr = ((px - lx) / cell).floor() as i32;
        let row_scr = ((py - ly) / cell).floor() as i32;

        let (col, row) = if self.board_flipped {
            ((self.layout.cols() as i32 - 1 - col_scr) as i8, row_scr as i8)
        } else {
            (col_scr as i8, (self.layout.rows() as i32 - 1 - row_scr) as i8)
        };

        if col < 0 || row < 0 || col >= self.layout.cols() as i8 || row >= self.layout.rows() as i8 {
            return None;
        }
        Some(Square::new(col, row))
    }

    // --- курсор ---

    pub(crate) fn update_cursor(&self, px: f32, py: f32) {
        let Some(w) = self.window.as_ref() else { return };

        let icon = if self.phase != Phase::Playing {
            CursorIcon::Default
        } else if self.waiting_for_opponent {
            if self.waiting_cancel_button_at(px, py) {
                CursorIcon::Grab
            } else {
                CursorIcon::Default
            }
        } else if self.disconnect_overlay.is_some() {
            if self.game_over_button_at(px, py) {
                CursorIcon::Grab
            } else {
                CursorIcon::Default
            }
        } else if self.game_over_button_at(px, py) {
            CursorIcon::Grab
        } else if self.dragging || self.dragging_reserve {
            CursorIcon::Grabbing
        } else if self.promotion_pending.is_some() {
            CursorIcon::Default
        } else if self.mouse_over_interactive(px, py) {
            CursorIcon::Grab
        } else {
            CursorIcon::Default
        };

        w.set_cursor(icon);
    }

    pub(crate) fn mouse_over_interactive(&self, px: f32, py: f32) -> bool {
        if self.waiting_for_opponent {
            return self.waiting_cancel_button_at(px, py);
        }

        if self.game_over_button_at(px, py) {
            return true;
        }

        if self.button_at(px, py).is_some() {
            return true;
        }

        if self.draw_offer_from.is_some() {
            if self.draw_overlay_button_at(px, py).is_some() {
                return true;
            }
        }

        if let Some(slot) = self.reserve_slot_at(px, py) {
            if Some(slot.color) == self.my_color {
                if let Some(st) = self.state.as_ref() {
                    if st.reserve.get(&slot.color).map_or(false, |v| v.contains(&slot.piece_type)) {
                        return true;
                    }
                }
            }
        }

        if let Some(cell) = self.pixel_to_cell(px, py) {
            if let Some(st) = self.state.as_ref() {
                if !st.is_over() {
                    if let Some(piece) = st.board.get(cell) {
                        if Some(piece.color) == self.my_color
                            && st.current_player == piece.color
                            {
                                return true;
                            }
                    }
                    if self.legal_moves.contains(&cell)
                        || self.legal_castles.contains(&cell)
                        || self.drop_moves.contains(&cell)
                        {
                            return true;
                        }
                }
            }
        }

        false
    }

    // --- часы ---

    pub(crate) fn current_clock(&self) -> Option<(Duration, Duration)> {
        let (mut white, mut black) = self.clock_local?;
        let synced = self.clock_synced_at?;
        let elapsed = Instant::now().saturating_duration_since(synced);

        let st = self.state.as_ref()?;
        if st.is_over() {
            return Some((white, black));
        }

        match st.current_player {
            Color::White => white = white.saturating_sub(elapsed),
            Color::Black => black = black.saturating_sub(elapsed),
        }
        Some((white, black))
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
        .with_title("Шахматы 16x10")
        .with_inner_size(winit::dpi::LogicalSize::new(1600.0, 1000.0))
        .with_resizable(true);

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                error!("[ui] create_window: {e}");
                event_loop.exit();
                return;
            }
        };

        let context = match Context::new(window.clone()) {
            Ok(c) => c,
            Err(e) => {
                error!("[ui] Context::new: {e}");
                event_loop.exit();
                return;
            }
        };

        let surface = match Surface::new(&context, window.clone()) {
            Ok(s) => s,
            Err(e) => {
                error!("[ui] Surface::new: {e}");
                event_loop.exit();
                return;
            }
        };

        let size = window.inner_size();
        self.window = Some(window.clone());
        self.context = Some(context);
        self.surface = Some(surface);

        info!("[ui] окно создано {}x{}", size.width, size.height);
        self.resize(size.width, size.height);

        window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                info!("[ui] CloseRequested");
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.resize(size.width, size.height);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let size = self.window.as_ref().map(|w| w.inner_size());
                if let Some(size) = size {
                    self.resize(size.width, size.height);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }


            WindowEvent::CursorMoved { position, .. } => {
                self.pointer_pos = (position.x, position.y);
                self.drag_pixel_x = position.x as f32;
                self.drag_pixel_y = position.y as f32;

                if self.dragging_hidden_royal.is_some() {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }

                let new_hover = self
                .button_at(position.x as f32, position.y as f32)
                .map(|b| b.kind);
                if new_hover != self.hovered_button {
                    self.hovered_button = new_hover;
                    self.hovered_since = new_hover.map(|_| Instant::now());
                    self.tooltip_shown = false;
                    self.bg_key = None;
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }

                if self.mouse_pressed && !self.dragging && !self.dragging_reserve {
                    let dx = self.drag_pixel_x - self.mouse_press_x;
                    let dy = self.drag_pixel_y - self.mouse_press_y;
                    if dx * dx + dy * dy > self.drag_threshold * self.drag_threshold {
                        if self.phase == Phase::Playing
                            && !self.waiting_for_opponent
                            && self.promotion_pending.is_none()
                            && self.disconnect_overlay.is_none()
                            {
                                if self.selected.is_some() {
                                    self.dragging = true;
                                    self.drag_from = self.selected;
                                    info!("[ui] начало drag из {:?}", self.selected);
                                } else if self.selected_reserve.is_some() {
                                    self.dragging_reserve = true;
                                    self.drag_from_reserve = self.selected_reserve;
                                    info!(
                                        "[ui] начало drag из запаса {:?}",
                                        self.selected_reserve
                                    );
                                }
                            }
                    }
                }

                if self.dragging || self.dragging_reserve {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }

                self.update_cursor(position.x as f32, position.y as f32);
            }



            WindowEvent::KeyboardInput { event, .. } => {
                if self.phase == Phase::Setup {
                    self.handle_setup_key(&event);
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button != MouseButton::Left {
                    return;
                }
                let (px, py) = (self.pointer_pos.0 as f32, self.pointer_pos.1 as f32);

                match state {
                    ElementState::Pressed => {
                        self.mouse_pressed = true;
                        self.mouse_press_x = px;
                        self.mouse_press_y = py;
                        self.drag_pixel_x = px;
                        self.drag_pixel_y = py;

                        match self.phase {
                            Phase::Setup => {
                                self.handle_setup_click(px, py);
                                if let Some(w) = &self.window {
                                    w.request_redraw();
                                }
                            }
                            Phase::Playing => {
                                self.handle_press_playing(px, py);
                            }
                        }
                    }
                    ElementState::Released => {
                        let was_dragging = self.dragging;
                        let was_dragging_reserve = self.dragging_reserve;
                        self.mouse_pressed = false;

                        if was_dragging {
                            if let Some(from) = self.drag_from {
                                if let Some(to) = self.pixel_to_cell(px, py) {
                                    let is_castle =
                                    self.legal_castles.iter().any(|m| *m == to);
                                    let is_move =
                                    self.legal_moves.iter().any(|m| *m == to);
                                    if is_castle || is_move {
                                        self.send_move(from, to, is_castle);
                                    }
                                }
                            }
                            self.dragging = false;
                            self.drag_from = None;
                            self.bg_key = None;
                            if let Some(w) = &self.window {
                                w.request_redraw();
                            }
                        } else if was_dragging_reserve {
                            let mut dropped = false;
                            if let Some((color, pt)) = self.drag_from_reserve {
                                if let Some(to) = self.pixel_to_cell(px, py) {
                                    if self.drop_moves.contains(&to) {
                                        self.send_drop(color, pt, to);
                                        dropped = true;
                                    }
                                }
                            }
                            self.dragging_reserve = false;
                            self.drag_from_reserve = None;
                            if !dropped {
                                self.clear_reserve_selection();
                            }
                            self.bg_key = None;
                            if let Some(w) = &self.window {
                                w.request_redraw();
                            }
                        }

                        // Обработка отпускания Royal'а из укрытия.
                        if let Some((pt, seat)) = self.dragging_hidden_royal {
                            if let Some(cell) = self.pixel_to_cell(px, py) {
                                if let Some(st) = self.state.as_ref() {
                                    let my_color = self.my_color.unwrap_or(Color::White);
                                    let valid = st.legal_royal_return_squares(my_color);
                                    if valid.contains(&cell) {
                                        if let Some(net) = self.net.as_ref() {
                                            let _ = net.cmd_tx.send(ClientCommand::ReturnRoyal { royal_pt: pt, to: cell });
                                        }
                                    }
                                }
                            }
                            self.dragging_hidden_royal = None;
                            self.bg_key = None;
                            if let Some(w) = &self.window { w.request_redraw(); }
                        }



                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.render() {
                    error!("[ui] render: {e:#}");
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let got = self.poll_net_events();
        if got {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }

        // ============================================================
        // Фикс 2: таймаут ожидания State (2 сек).
        // Если State не пришёл за 2 секунды — снимаем блокировку,
        // чтобы бот не завис навсегда при потере пакета.
        // ============================================================
        if let Some(since) = self.waiting_for_state_since {
            if since.elapsed() > Duration::from_secs(2) {
                warn!("[ui] State не пришёл за 2 сек — разблокируем бота");
                self.waiting_for_state = false;
                self.waiting_for_state_since = None;
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }

        #[cfg(debug_assertions)]
        if self.bot_enabled {
            self.try_bot_move();
        }

        // ============================================================
        // Повторная отправка InvitePartner, пока партнёр не появился
        // и команда не сформирована.
        // ============================================================
        if let Some(at) = self.invite_retry_at {
            if Instant::now() >= at {
                if let (Some(net), Some(target)) =
                    (self.net.as_ref(), self.invite_target.as_ref())
                    {
                        let _ = net.cmd_tx.send(ClientCommand::InvitePartner {
                            target_name: target.clone(),
                        });
                        info!("[ui] повторное приглашение {target}");
                    }
                    self.invite_retry_at = Some(Instant::now() + Duration::from_secs(2));
            }
        }

        // ============================================================
        // Таймеры: сдача, ничья, transient-сообщения.
        // ============================================================

        if let Some(deadline) = self.resign_request_deadline {
            if Instant::now() >= deadline {
                if let Some(net) = self.net.as_ref() {
                    let _ = net.cmd_tx.send(ClientCommand::OfferResignDecline);
                }
                self.resign_request_from_partner = None;
                self.resign_request_deadline = None;
                self.bg_key = None;
            }
        }

        if let Some(deadline) = self.draw_offer_deadline {
            if Instant::now() >= deadline {
                self.draw_offer_from = None;
                self.draw_offer_deadline = None;
                self.bg_key = None;
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }

        if let Some((_, until)) = self.transient_message.as_ref() {
            if Instant::now() >= *until {
                self.transient_message = None;
                self.bg_key = None;
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }

        if self.draw_offer_from.is_some() || self.transient_message.is_some() {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }

        // Таймер запроса партнёра (фаза 1 ничьей в 2×2).
        if let Some(deadline) = self.draw_request_deadline {
            if Instant::now() >= deadline {
                if let Some(net) = self.net.as_ref() {
                    let _ = net.cmd_tx.send(ClientCommand::OfferDrawDecline);
                }
                self.draw_request_from_partner = None;
                self.draw_request_deadline = None;
                self.bg_key = None;
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }

        // Таймер предложения соперников (фаза 2 ничьей в 2×2).
        if let Some(ref offer) = self.draw_offer_2v2 {
            if Instant::now() >= offer.deadline && !offer.accepted_by_me {
                if let Some(net) = self.net.as_ref() {
                    let _ = net.cmd_tx.send(ClientCommand::DeclineDraw);
                }
                self.draw_offer_2v2 = None;
                self.bg_key = None;
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }

        if self.draw_request_from_partner.is_some() || self.draw_offer_2v2.is_some() {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }

        // ============================================================
        // Интервал опроса.
        // ============================================================
        let has_running_clock = self
        .state
        .as_ref()
        .map_or(false, |st| st.clock.is_some() && !st.is_over());

        let mut interval = if self.dragging || self.dragging_reserve {
            DRAG_POLL_INTERVAL
        } else if has_running_clock {
            CLOCK_POLL_INTERVAL
        } else {
            NET_POLL_INTERVAL
        };

        // Тултип: если мышь над кнопкой, не спим дольше до TOOLTIP_DELAY.
        if let (Some(_), Some(since)) = (self.hovered_button, self.hovered_since) {
            let elapsed = since.elapsed();
            if elapsed < TOOLTIP_DELAY {
                let wait = TOOLTIP_DELAY - elapsed;
                if wait < interval {
                    interval = wait;
                }
            } else if !self.tooltip_shown {
                self.tooltip_shown = true;
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }

        // Retry-приглашение: не спим дольше, чем до него.
        if let Some(at) = self.invite_retry_at {
            let until = at.saturating_duration_since(Instant::now());
            if until < interval {
                interval = until;
            }
        }

        // Фикс 2: не спим дольше, чем до таймаута waiting_for_state.
        if let Some(since) = self.waiting_for_state_since {
            let until = Duration::from_secs(2).saturating_sub(since.elapsed());
            if until < interval {
                interval = until;
            }
        }

        if has_running_clock {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }

        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + interval));
    }

}
