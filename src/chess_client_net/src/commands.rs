//! Команды из UI в сеть.

use chess_logic::{PieceType, Seat, Square};

/// Команда, которую UI/CLI шлёт в сетевой поток.
#[derive(Debug, Clone)]
pub enum ClientCommand {
    // --- Duel ---
    MakeMove {
        from: Square,
        to: Square,
        promotion: Option<PieceType>,
    },
    Castle { a: Square, c: Square },
    Drop { piece: PieceType, to: Square },
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

    OfferResignRequest,
    OfferResignConfirm,
    OfferResignDecline,

    // --- 2x2: лобби / приглашения ---
    InvitePartner { target_name: String },
    CancelInvite,
    AcceptInvite { from_name: String },
    DeclineInvite { from_name: String },
    LeaveLobby,

    // --- 2x2: двухфазная ничья ---
    OfferDrawRequest,
    OfferDrawConfirm,
    OfferDrawDecline,

    // --- Общее ---
    Shutdown,
}

/// Режим, с которым клиент стартует игру.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientMode {
    Duel,
    Team2v2Random,
    Team2v2Pick,
}

impl ClientMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ClientMode::Duel => "duel",
            ClientMode::Team2v2Random => "team_2v2_random",
            ClientMode::Team2v2Pick => "team_2v2_pick",
        }
    }
}

/// Хелпер: узнать Seat по Welcome.
pub fn seat_of(welcome_seat: Option<Seat>, default: Seat) -> Seat {
    welcome_seat.unwrap_or(default)
}
