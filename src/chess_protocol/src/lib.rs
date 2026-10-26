//! Протокол сетевого обмена между клиентом и сервером.

use chess_logic::{Color, EndReason, GameState, PieceType, Seat, Square};
use serde::{Deserialize, Serialize};

// ============================================================
// Клиент → Сервер
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ClientMessage {
    Hello(Hello),

    /// Игрок предлагает партнёру сдаться (фаза 1, 2х2).
    OfferResignRequest,
    /// Партнёр согласен сдаться. 2х2
    OfferResignConfirm,
    /// Партнёр отказался сдаваться. 2х2
    OfferResignDecline,

    // --- Duel ---
    MakeMove {
        from: Square,
        to: Square,
        #[serde(skip_serializing_if = "Option::is_none")]
        promotion: Option<PieceType>,
    },
    Castle {
        a: Square,
        c: Square,
    },
    Drop {
        piece: PieceType,
        to: Square,
    },
    OfferDraw,
    AcceptDraw,
    DeclineDraw,
    Resign,
    RequestState,

    // --- 2x2: возврат royal'а ---
    ReturnRoyal {
        royal_pt: PieceType,
        to: Square,
    },

    // --- 2x2: лобби / приглашения ---
    /// Пригласить партнёра по имени (только в team_2v2_pick).
    InvitePartner {
        target_name: String,
    },
    CancelInvite,
    AcceptInvite {
        from_name: String,
    },
    DeclineInvite {
        from_name: String,
    },
    LeaveLobby,

    // --- 2x2: двухфазная ничья ---
    /// Игрок предлагает своему партнёру инициировать предложение ничьей.
    OfferDrawRequest,
    /// Партнёр согласен.
    OfferDrawConfirm,
    /// Партнёр отказался.
    OfferDrawDecline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub name: String,
    pub variant: String,
    pub time_control: String,
    /// "duel" | "team_2v2_random" | "team_2v2_pick" | "team_2v2_explicit"
    pub mode: String,
    /// Для 1×1 — имя конкретного соперника.
    /// Для `team_2v2_explicit` — два имени через запятую: "1,2" или "2,1".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opponent_name: Option<String>,
    /// Для `team_2v2_pick` и `team_2v2_explicit` — имя партнёра.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partner_name: Option<String>,
}

// ============================================================
// Сервер → Клиент
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ServerMessage {

    // В enum ServerMessage добавь:
    History { snapshots: Vec<chess_logic::state::HistorySnapshot> },

    /// Партнёр предлагает сдаться. 2х2
    ResignRequestToPartner { from_name: String },
    /// Партнёр отказался сдаваться. 2х2
    ResignRequestDeclined,
    /// Сторона сдалась. 2х2
    TeamResigned { by_color: Color },

    Welcome(Welcome),
    State(GameState),
    Error {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
    GameOver {
        reason: EndReason,
    },
    DrawOffer {
        by: Color,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by_names: Option<[String; 2]>,
    },
    DrawDeclined,
    OpponentDisconnect,
    ConnectionLost,
    PartnerLeft {
        partner_name: String,
    },

    // --- 2x2: приглашения ---
    InviteSent {
        to_name: String,
    },
    InviteReceived {
        from_name: String,
        variant: String,
        time_control: String,
    },
    InviteCancelled {
        from_name: String,
    },
    InviteDeclined {
        by_name: String,
    },
    InviteError {
        code: String,
        message: String,
    },
    TeamFormed {
        partner_name: String,
        you_are: Seat,
    },

    // --- 2x2: двухфазная ничья ---
    /// Партнёр предлагает инициировать ничью.
    DrawRequestToPartner {
        from_name: String,
    },
    /// Партнёр отказался от инициирования.
    DrawRequestDeclined,
    /// Один из соперников согласился, ждём второго.
    DrawOfferAccepted{
        by_color: Color,
        by_seat: Seat,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Welcome {
    pub color: Color,
    pub game_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opponent_name: Option<String>,

    // --- 2x2 ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat: Option<Seat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partner_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opponents: Option<[String; 2]>,
}

// ============================================================
// Реэкспорт GameMode для удобства клиентов
// ============================================================

pub use chess_logic::GameMode;

// ============================================================
// Тесты
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_hello_roundtrip() {
        let msg = ClientMessage::Hello(Hello {
            version: Some("0.1".into()),
                                       name: "Александр".into(),
                                       variant: "10x10".into(),
                                       time_control: "300+3".into(),
                                       mode: "duel".into(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::Hello(h) => {
                assert_eq!(h.name, "Александр");
                assert_eq!(h.mode, "duel");
            }
            _ => panic!("не hello"),
        }
    }

    #[test]
    fn client_return_royal_roundtrip() {
        let msg = ClientMessage::ReturnRoyal {
            royal_pt: PieceType::Prince,
            to: Square::new(3, 0),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::ReturnRoyal { royal_pt, to } => {
                assert_eq!(royal_pt, PieceType::Prince);
                assert_eq!(to, Square::new(3, 0));
            }
            _ => panic!("не return_royal"),
        }
    }

    #[test]
    fn client_invite_roundtrip() {
        let msg = ClientMessage::InvitePartner {
            target_name: "Вася".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::InvitePartner { target_name } => assert_eq!(target_name, "Вася"),
            _ => panic!("не invite_partner"),
        }
    }

    #[test]
    fn server_welcome_duel_roundtrip() {
        let msg = ServerMessage::Welcome(Welcome {
            color: Color::White,
            game_id: "g1".into(),
                                         opponent_name: Some("Борис".into()),
                                         seat: None,
                                         partner_name: None,
                                         opponents: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Welcome(w) => {
                assert_eq!(w.color, Color::White);
                assert!(w.seat.is_none());
            }
            _ => panic!("не welcome"),
        }
    }

    #[test]
    fn server_welcome_2v2_roundtrip() {
        let msg = ServerMessage::Welcome(Welcome {
            color: Color::White,
            game_id: "g2".into(),
                                         opponent_name: None,
                                         seat: Some(Seat::First),
                                         partner_name: Some("Петя".into()),
                                         opponents: Some(["Маша".into(), "Оля".into()]),
        });
        let json = serde_json::to_string(&msg).unwrap();
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Welcome(w) => {
                assert_eq!(w.seat, Some(Seat::First));
                assert_eq!(w.partner_name.as_deref(), Some("Петя"));
                assert_eq!(w.opponents.as_ref().unwrap()[0], "Маша");
            }
            _ => panic!("не welcome"),
        }
    }

    #[test]
    fn server_team_formed_roundtrip() {
        let msg = ServerMessage::TeamFormed {
            partner_name: "Петя".into(),
            you_are: Seat::Second,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::TeamFormed { partner_name, you_are } => {
                assert_eq!(partner_name, "Петя");
                assert_eq!(you_are, Seat::Second);
            }
            _ => panic!("не team_formed"),
        }
    }

    #[test]
    fn server_error_with_code() {
        let msg = ServerMessage::Error {
            message: "имя занято".into(),
            code: Some("name_taken".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Error { message, code } => {
                assert_eq!(message, "имя занято");
                assert_eq!(code.as_deref(), Some("name_taken"));
            }
            _ => panic!("не error"),
        }
    }
}
