//! События из сети в UI.

use chess_logic::{Color, EndReason, GameState, Seat};

/// Событие, которое сетевой поток шлёт в UI.
#[derive(Debug, Clone)]
pub enum ClientEvent {
    History { snapshots: Vec<chess_logic::state::HistorySnapshot> },
    /// Соединение установлено, Hello отправлен.
    Connected,

    /// Пришёл Welcome.
    Welcome {
        color: Color,
        game_id: String,
        opponent_name: Option<String>,
        // --- 2x2 ---
        seat: Option<Seat>,
        partner_name: Option<String>,
        opponents: Option<[String; 2]>,
    },

    /// Новое полное состояние игры.
    State(GameState),

    /// Партия окончена.
    GameOver { reason: EndReason },

    /// Ошибка от сервера.
    ServerError { message: String, code: Option<String> },

    /// Ничья (duel).
    DrawOffer {
        by: Color,
        by_names: Option<[String; 2]>,
    },
    DrawDeclined,

    DrawOfferAccepted { by_color: Color, by_seat: Seat },

    /// 2x2: приглашения.
    InviteSent { to_name: String },
    InviteReceived {
        from_name: String,
        variant: String,
        time_control: String,
    },
    InviteCancelled { from_name: String },
    InviteDeclined { by_name: String },
    InviteError { code: String, message: String },
    TeamFormed { partner_name: String, you_are: Seat },
    PartnerLeft { partner_name: String },

    ResignRequestToPartner { from_name: String },
    ResignRequestDeclined,
    TeamResigned { by_color: Color },

    /// 2x2: двухфазная ничья.
    DrawRequestToPartner { from_name: String },
    DrawRequestDeclined,

    /// Соперник отключился (сервер жив).
    OpponentDisconnected,

    /// Соединение с сервером потеряно.
    ConnectionLost { reason: String },
}
