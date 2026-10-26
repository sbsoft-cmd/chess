//! Экран настройки: имя, соперник, союзник, вариант, время.

use crate::layout::{Layout, LEFT_PANEL, MARGIN, RIGHT_PANEL, TOP_MARGIN};

// ============================================================
// Время
// ============================================================

pub struct TimeOption {
    pub label: &'static str,
    pub value: &'static str,
}

pub const TIME_OPTIONS: &[TimeOption] = &[
    TimeOption { label: "Без контроля времени",   value: "none" },
TimeOption { label: "3 минуты",               value: "180+0" },
TimeOption { label: "3 минуты + 2 секунды",   value: "180+2" },
TimeOption { label: "5 минут",                value: "300+0" },
TimeOption { label: "5 минут + 3 секунды",    value: "300+3" },
TimeOption { label: "10 минут",               value: "600+0" },
TimeOption { label: "10 минут + 5 секунд",    value: "600+5" },
TimeOption { label: "15 минут",               value: "900+0" },
TimeOption { label: "15 минут + 10 секунд",   value: "900+10" },
TimeOption { label: "30 минут",               value: "1800+0" },
TimeOption { label: "30 минут + 15 секунд",   value: "1800+15" },
];

// ============================================================
// Варианты
// ============================================================

pub const VARIANT_8X8: &str = "8x8";
pub const VARIANT_8X8_FREESTYLE: &str = "8x8-freestyle";
pub const VARIANT_10X10: &str = "10x10";
pub const VARIANT_10X10_FREESTYLE: &str = "10x10-freestyle";
pub const VARIANT_16X10: &str = "16x10";           // 2×2 с выбранным союзником


// ============================================================
// Роль (только debug)
// ============================================================

#[cfg(debug_assertions)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Human,
    Bot,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum LobbyView {
    #[default]
    Idle,
    InviteSent { to_name: String },
    InviteReceived {
        from_name: String,
        variant: String,
        time_control: String,
    },
    InPair { partner_name: String },
}











// ============================================================
// Фокус поля
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusField {
    Name,
    Opponent,
    Partner,
    None,
}

// ============================================================
// Состояние
// ============================================================

pub struct SetupState {
    pub name: String,
    pub opponent_name: String,
    pub partner_name: String,
    pub variant: &'static str,
    pub time_control: &'static str,
    #[cfg(debug_assertions)]
    pub role: Role,
    pub focused_field: FocusField,
    pub lobby: LobbyView, // <-- добавить
}

fn parse_opponents(s: &str) -> Option<[String; 2]> {
    let parts: Vec<&str> = s
    .split(',')
    .map(|x| x.trim())
    .filter(|x| !x.is_empty())
    .collect();
    if parts.len() != 2 {
        return None;
    }
    let a = parts[0].to_string();
    let b = parts[1].to_string();
    if a.eq_ignore_ascii_case(&b) {
        return None;
    }
    Some([a, b])
}

fn opponents_eq(a: &[String], b: &[String]) -> bool {
    if a.len() != 2 || b.len() != 2 {
        return false;
    }
    let a0 = a[0].to_lowercase();
    let a1 = a[1].to_lowercase();
    let b0 = b[0].to_lowercase();
    let b1 = b[1].to_lowercase();
    (a0 == b0 && a1 == b1) || (a0 == b1 && a1 == b0)
}

impl SetupState {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            opponent_name: String::new(),
            partner_name: String::new(),
            variant: VARIANT_8X8,
            time_control: "none",
            #[cfg(debug_assertions)]
            role: Role::Human,
            focused_field: FocusField::Name,
            lobby: LobbyView::Idle,
        }
    }

    pub fn can_start(&self) -> bool {
        if self.name.trim().is_empty() {
            return false;
        }
        // explicit: если заполнены и partner, и opponent — формат opponent должен быть "A,B".
        if self.variant == VARIANT_16X10
            && !self.partner_name.trim().is_empty()
            && !self.opponent_name.trim().is_empty()
            {
                return parse_opponents(&self.opponent_name).is_some();
            }
            true
    }

    pub fn is_team(&self) -> bool {
        self.variant == VARIANT_16X10
    }

    pub fn mode_str(&self) -> &'static str {
        match self.variant {
            VARIANT_8X8 => "classic_8x8",
            VARIANT_8X8_FREESTYLE => "classic_8x8_freestyle",
            VARIANT_10X10 => "duel",
            VARIANT_10X10_FREESTYLE => "duel_freestyle",
            VARIANT_16X10 => {
                let has_partner = !self.partner_name.trim().is_empty();
                let has_opponents = !self.opponent_name.trim().is_empty();
                if has_partner && has_opponents {
                    "team_2v2_explicit"
                } else if has_partner {
                    "team_2v2_pick"
                } else {
                    "team_2v2_random"
                }
            }
            _ => "duel",

        }
    }

    pub fn push_char(&mut self, ch: char) {
        let is_opponent = self.focused_field == FocusField::Opponent;
        let (field, max_len) = match self.focused_field {
            FocusField::Name => (&mut self.name, 20),
            FocusField::Opponent => (&mut self.opponent_name, 40), // больше для "1,2"
            FocusField::Partner => (&mut self.partner_name, 20),
            FocusField::None => return,
        };
        if field.chars().count() >= max_len {
            return;
        }
        // Разрешаем буквы, цифры, пробел, подчёркивание; в поле «соперник» — ещё запятую.
        let allowed = ch.is_alphanumeric()
        || ch == ' '
        || ch == '_'
        || (is_opponent && ch == ',');
        if allowed {
            field.push(ch);
        }
    }

    pub fn pop_char(&mut self) {
        match self.focused_field {
            FocusField::Name => { self.name.pop(); }
            FocusField::Opponent => { self.opponent_name.pop(); }
            FocusField::Partner => { self.partner_name.pop(); }
            FocusField::None => {}
        }
    }
}

impl Default for SetupState {
    fn default() -> Self { Self::new() }
}

// ============================================================
// Rect / SetupLayout
// ============================================================

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

pub struct SetupLayout {
    pub name_field: Rect,
    pub opponent_field: Rect,
    pub partner_field: Rect,

    pub variant_8: Rect,
    pub variant_8_free: Rect,
    pub variant_10: Rect,
    pub variant_10_free: Rect,
    pub variant_16: Rect,

    #[cfg(debug_assertions)]
    pub role_human: Rect,
    #[cfg(debug_assertions)]
    pub role_bot: Rect,

    pub time_rects: Vec<Rect>,
    pub play_button: Rect,
}

impl SetupLayout {
    pub fn from_window(width: u32, height: u32) -> Self {
        let w = width as f32;
        let h = height as f32;
        let cx = w / 2.0;

        // ============================================================
        // 1. Поля имён (3 в ряд): имя, соперник, союзник.
        // ============================================================
        let field_w = 220.0;
        let field_h = 44.0;
        let field_gap = 16.0;
        let field_y = 130.0;
        let total_field_w = field_w * 3.0 + field_gap * 2.0;
        let field_start = cx - total_field_w / 2.0;
        let name_field = Rect { x: field_start, y: field_y, w: field_w, h: field_h };
        let opponent_field = Rect {
            x: field_start + field_w + field_gap,
            y: field_y,
            w: field_w,
            h: field_h,
        };
        let partner_field = Rect {
            x: field_start + 2.0 * (field_w + field_gap),
            y: field_y,
            w: field_w,
            h: field_h,
        };

        // ============================================================
        // 2. Вариант (5 кнопок в ряд).
        // ============================================================
        let v_w = 130.0;
        let v_h = 40.0;
        let v_gap = 10.0;
        let v_y = 210.0;
        let v_total = v_w * 5.0 + v_gap * 4.0;
        let v_start = cx - v_total / 2.0;

        let variant_8       = Rect { x: v_start, y: v_y, w: v_w, h: v_h };
        let variant_8_free  = Rect { x: v_start + 1.0 * (v_w + v_gap), y: v_y, w: v_w, h: v_h };
        let variant_10      = Rect { x: v_start + 2.0 * (v_w + v_gap), y: v_y, w: v_w, h: v_h };
        let variant_10_free = Rect { x: v_start + 3.0 * (v_w + v_gap), y: v_y, w: v_w, h: v_h };
        let variant_16      = Rect { x: v_start + 4.0 * (v_w + v_gap), y: v_y, w: v_w, h: v_h };

        // ============================================================
        // 3. Время: 1 + 2×5 = 11 кнопок.
        // ============================================================
        #[cfg(debug_assertions)]
        let t_top = 290.0;
        #[cfg(not(debug_assertions))]
        let t_top = 280.0;

        let time_w = 440.0;
        let time_h = 36.0;
        let time_row_gap = 6.0;
        let time_gap = 8.0;

        let mut time_rects = Vec::new();

        // Ряд 3: одна кнопка по центру («Без контроля времени»).
        let y = t_top;
        time_rects.push(Rect { x: cx - time_w / 2.0, y, w: time_w, h: time_h });

        // Ряды 4-8: по 2 кнопки в ряд.
        let half_w = (time_w - time_gap) / 2.0;
        for i in 0..5 {
            let yy = t_top + (i as f32 + 1.0) * (time_h + time_row_gap);
            time_rects.push(Rect { x: cx - time_w / 2.0, y: yy, w: half_w, h: time_h });
            time_rects.push(Rect {
                x: cx - time_w / 2.0 + half_w + time_gap,
                y: yy,
                w: half_w,
                h: time_h,
            });
        }

        // ============================================================
        // 4. Роль (9-й ряд): Человек / Бот.
        // ============================================================
        #[cfg(debug_assertions)]
        let (role_human, role_bot) = {
            let role_y = t_top + 6.0 * (time_h + time_row_gap) + 20.0;
            let role_w = half_w;
            let role_h = time_h;
            (
                Rect { x: cx - time_w / 2.0, y: role_y, w: role_w, h: role_h },
             Rect {
                 x: cx - time_w / 2.0 + half_w + time_gap,
                 y: role_y,
                 w: role_w,
                 h: role_h,
             },
            )
        };

        // ============================================================
        // 5. Кнопка «Играть».
        // ============================================================
        let p_w = 220.0;
        let p_h = 52.0;
        let play_button = Rect { x: cx - p_w / 2.0, y: h - 80.0, w: p_w, h: p_h };

        let _ = (
            Layout::from_window(width, height, chess_logic::GameMode::Duel),
                 LEFT_PANEL,
                 RIGHT_PANEL,
                 MARGIN,
                 TOP_MARGIN,
        );

        Self {
            name_field,
            opponent_field,
            partner_field,
            variant_8,
            variant_8_free,
            variant_10,
            variant_10_free,
            variant_16,
            #[cfg(debug_assertions)]
            role_human,
            #[cfg(debug_assertions)]
            role_bot,
            time_rects,
            play_button,
        }

    }
}
