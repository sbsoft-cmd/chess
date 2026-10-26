//! Состояние стороны и игроков в 2-на-2.

use crate::piece::{PieceType, Seat};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PlayerState {
    pub alive: bool,

    // Royal'ы бессмертны: true означает "в укрытии" (на запасном поле).
    pub prince_in_holding: bool,
    pub princess_in_holding: bool,

    // Сколько ходов осталось ждать до возврата (0 = можно вернуть).
    pub prince_cd: u8,
    pub princess_cd: u8,

    // Сколько раз рубили (для расчета кулдауна).
    pub prince_hits: u8,
    pub princess_hits: u8,
}

impl PlayerState {
    pub fn new() -> Self {
        Self {
            alive: true,
            prince_in_holding: false,
            princess_in_holding: false,
            prince_cd: 0,
            princess_cd: 0,
            prince_hits: 0,
            princess_hits: 0,
        }
    }

    /// Находится ли Royal в укрытии.
    pub fn is_in_holding(&self, pt: PieceType) -> bool {
        match pt {
            PieceType::Prince => self.prince_in_holding,
            PieceType::Princess => self.princess_in_holding,
            _ => false,
        }
    }

    /// Возвращает оставшийся кулдаун (сколько ходов ждать).
    pub fn get_cd(&self, pt: PieceType) -> u8 {
        match pt {
            PieceType::Prince => self.prince_cd,
            PieceType::Princess => self.princess_cd,
            _ => 0,
        }
    }

    /// Возвращает тип Royal'а, которого нужно вернуть прямо сейчас (cd == 0).
    pub fn must_return_royal(&self) -> Option<PieceType> {
        if self.prince_in_holding && self.prince_cd == 0 {
            return Some(PieceType::Prince);
        }
        if self.princess_in_holding && self.princess_cd == 0 {
            return Some(PieceType::Princess);
        }
        None
    }

    /// Игрок должен пропустить ход, если его Royal в укрытии и еще нельзя вернуть (cd > 0).
    pub fn must_skip_turn(&self) -> bool {
        let prince_wait = self.prince_in_holding && self.prince_cd > 0;
        let princess_wait = self.princess_in_holding && self.princess_cd > 0;

        // Если оба в укрытии и оба ждут — точно пропуск.
        if self.prince_in_holding && self.princess_in_holding {
            return prince_wait && princess_wait;
        }
        // Если один в укрытии и он ждет — пропуск.
        prince_wait || princess_wait
    }

    /// Может ли игрок сделать ход (не пропускает, не выбыл).
    pub fn is_active(&self) -> bool {
        if !self.alive {
            return false;
        }
        // Если Royal в укрытии и ждет (cd > 0) — игрок пропускает ход.
        if self.must_skip_turn() {
            return false;
        }
        true
    }
}

impl Default for PlayerState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TeamState {
    pub moves_made_this_turn: u8,
    pub last_seat_moved: Option<Seat>, // <-- Кто сделал последний ход в этом окне
    pub players: [PlayerState; 2],
}

impl TeamState {
    pub fn new() -> Self {
        Self {
            moves_made_this_turn: 0,
            last_seat_moved: None,
            players: [PlayerState::new(), PlayerState::new()],
        }

    }

    pub fn player(&self, seat: Seat) -> &PlayerState {
        &self.players[seat.index()]
    }

    pub fn player_mut(&mut self, seat: Seat) -> &mut PlayerState {
        &mut self.players[seat.index()]
    }

    pub fn any_alive(&self) -> bool {
        self.players.iter().any(|p| p.alive)
    }

    pub fn reset_turn_counters(&mut self) {
        self.moves_made_this_turn = 0;
        self.last_seat_moved = None;
    }

    /// Может ли конкретный игрок ходить прямо сейчас.
    pub fn is_player_active(&self, seat: Seat) -> bool {
        self.player(seat).is_active()
    }

    /// Сколько игроков в команде могут сделать ход (активны).
    pub fn active_players_count(&self) -> u8 {
        Seat::all().iter().filter(|s| self.is_player_active(**s)).count() as u8
    }
}

impl Default for TeamState {
    fn default() -> Self {
        Self::new()
    }
}
