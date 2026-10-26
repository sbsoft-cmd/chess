//! Сериализация GameState в JSON.
//!
//! Board — объект {cols, rows, cells: [{col, row, color, type, owner?}]}.
//! MovedPawns — список [{col, row, val}].
//! Reincarnating — map[Color][]ReincarnatingPawn.
//! Clock — кастомный формат (Instant не сериализуется напрямую).
//!
//! ВАЖНО: `history` НЕ сериализуется в `GameState` (в `State`-сообщении).
//! История передаётся отдельным `ServerMessage::History { snapshots }`
//! только в конце игры. Иначе `State` растёт линейно с числом ходов
//! и превышает MAX_FRAME (1 МиБ), из-за чего writer клиента падает
//! с FrameTooLong.

use crate::board::Board;
use crate::clock::Clock;
use crate::moves::EnPassantMove;
use crate::piece::{Color, Piece, PieceType, Seat, Square};
use crate::reincarnation::ReincarnatingPawn;
use crate::state::{EndReason, GameMode, GameState};
use crate::team::TeamState;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

// ============================================================
// Board
// ============================================================

#[derive(Serialize, Deserialize)]
struct BoardEntry {
    col: i8,
    row: i8,
    color: Color,
    #[serde(rename = "type")]
    piece_type: PieceType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner: Option<Seat>,
}

#[derive(Serialize, Deserialize)]
struct BoardJson {
    cols: i8,
    rows: i8,
    cells: Vec<BoardEntry>,
}

impl Serialize for Board {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let entries: Vec<BoardEntry> = self
        .iter_pieces()
        .map(|(sq, p)| BoardEntry {
            col: sq.col,
            row: sq.row,
            color: p.color,
            piece_type: p.piece_type,
            owner: self.get_owner(sq),
        })
        .collect();
        let json = BoardJson {
            cols: self.cols(),
            rows: self.rows(),
            cells: entries,
        };
        json.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Board {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let json = BoardJson::deserialize(d)?;
        let mut board = Board::empty_with_size(json.cols, json.rows);
        for e in json.cells {
            board.set_with_owner(
                Square::new(e.col, e.row),
                                 Some(Piece::new(e.color, e.piece_type)),
                                 e.owner,
            );
        }
        Ok(board)
    }
}

// ============================================================
// MovedPawns (HashSet<Square>) — как список с флагом val
// ============================================================

#[derive(Serialize, Deserialize)]
struct SquareBoolEntry {
    col: i8,
    row: i8,
    val: bool,
}

// ============================================================
// Clock — Instant не сериализуется напрямую.
// ============================================================

#[derive(Serialize, Deserialize)]
struct ClockJson {
    white_ms: u64,
    black_ms: u64,
    increment_ms: u64,
    #[serde(default)]
    last_move_elapsed_ms: u64,
}

impl Serialize for Clock {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let cj = ClockJson {
            white_ms: self.white.as_millis() as u64,
            black_ms: self.black.as_millis() as u64,
            increment_ms: self.increment.as_millis() as u64,
            last_move_elapsed_ms: self.last_move.elapsed().as_millis() as u64,
        };
        cj.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Clock {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let cj = ClockJson::deserialize(d)?;
        let mut c = Clock::new(
            Duration::from_millis(cj.white_ms),
                               Duration::from_millis(cj.increment_ms),
        );
        c.white = Duration::from_millis(cj.white_ms);
        c.black = Duration::from_millis(cj.black_ms);
        c.last_move = std::time::Instant::now()
        .checked_sub(Duration::from_millis(cj.last_move_elapsed_ms))
        .unwrap_or_else(std::time::Instant::now);
        Ok(c)
    }
}

// ============================================================
// GameState
// ============================================================
//
// ВАЖНО: поле `history` сознательно ОТСУТСТВУЕТ в GameStateJson.
// Причина: `State` шлётся после каждого хода, а `HistorySnapshot`
// содержит полную доску + teams + reserve + reincarnating.
// Через N ходов `State` превышал MAX_FRAME (1 МиБ), writer падал
// с FrameTooLong, и сессия «зависала» (клиенты не получали State).
//
// История передаётся отдельным `ServerMessage::History { snapshots }`
// только в конце игры.

#[derive(Serialize, Deserialize)]
struct GameStateJson {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<GameMode>,
    position: Board,
    current_player: Color,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    moved_pawns: Vec<SquareBoolEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    en_passant: Option<EnPassantMove>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    winner: Option<Color>,
    #[serde(default, skip_serializing_if = "is_false")]
    is_draw: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resigned_by: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    end_reason: Option<EndReason>,
    #[serde(default, skip_serializing_if = "is_empty_map")]
    reserve: HashMap<Color, Vec<PieceType>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clock: Option<Clock>,
    #[serde(default, skip_serializing_if = "is_false")]
    first_move_made: bool,
    #[serde(default, skip_serializing_if = "is_empty_map")]
    reincarnating: HashMap<Color, Vec<ReincarnatingPawn>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    white_team: Option<TeamState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    black_team: Option<TeamState>,
    // history — НЕ сериализуется. См. комментарий выше.
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_empty_map<K, V>(m: &HashMap<K, V>) -> bool {
    m.is_empty()
}

impl Serialize for GameState {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let moved: Vec<SquareBoolEntry> = self
        .moved_pawns
        .iter()
        .map(|sq| SquareBoolEntry {
            col: sq.col,
            row: sq.row,
            val: true,
        })
        .collect();

        let json = GameStateJson {
            mode: Some(self.mode),
            position: self.board.clone(),
            current_player: self.current_player,
            moved_pawns: moved,
            en_passant: self.en_passant.clone(),
            winner: self.winner,
            is_draw: self.is_draw,
            resigned_by: self.resigned_by,
            end_reason: self.end_reason,
            reserve: self.reserve.clone(),
            clock: self.clock.clone(),
            first_move_made: self.first_move_made,
            reincarnating: self.reincarnating.clone(),
            white_team: Some(self.white_team.clone()),
            black_team: Some(self.black_team.clone()),
            // history: НЕ передаём.
        };
        json.serialize(s)
    }
}

impl<'de> Deserialize<'de> for GameState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let json = GameStateJson::deserialize(d)?;

        let mut moved_pawns: HashSet<Square> = HashSet::new();
        for e in json.moved_pawns {
            if e.val {
                moved_pawns.insert(Square::new(e.col, e.row));
            }
        }

        Ok(GameState {
            mode: json.mode.unwrap_or(GameMode::Duel),
           board: json.position,
           current_player: json.current_player,
           moved_pawns,
           en_passant: json.en_passant,
           winner: json.winner,
           is_draw: json.is_draw,
           resigned_by: json.resigned_by,
           end_reason: json.end_reason,
           reserve: json.reserve,
           clock: json.clock,
           first_move_made: json.first_move_made,
           reincarnating: json.reincarnating,
           white_team: json.white_team.unwrap_or_default(),
           black_team: json.black_team.unwrap_or_default(),
           // history при десериализации пустая — она приходит отдельным
           // ServerMessage::History только в конце игры.
           history: Vec::new(),
        })
    }
}

// ============================================================
// Тесты
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_roundtrip() {
        let b = Board::initial();
        let json = serde_json::to_string(&b).unwrap();
        let back: Board = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn board_json_format() {
        let b = Board::initial();
        let json = serde_json::to_string(&b).unwrap();
        assert!(json.starts_with('{'), "должен быть объект: {json}");
        assert!(json.contains(r#""cols""#));
        assert!(json.contains(r#""rows""#));
        assert!(json.contains(r#""cells""#));
        assert!(json.contains(r#""color":"w""#));
        assert!(json.contains(r#""type":"K""#));
        assert!(json.contains(r#""type":"N2""#));
    }

    #[test]
    fn game_state_roundtrip_initial() {
        let g = GameState::new();
        let json = serde_json::to_string(&g).unwrap();
        let back: GameState = serde_json::from_str(&json).unwrap();

        assert_eq!(g.board, back.board);
        assert_eq!(g.current_player, back.current_player);
        assert_eq!(g.moved_pawns, back.moved_pawns);
        assert!(g.en_passant.is_none() && back.en_passant.is_none());
        assert!(g.winner.is_none() && back.winner.is_none());
        assert!(g.clock.is_none() && back.clock.is_none());
        assert_eq!(g.first_move_made, back.first_move_made);
    }

    #[test]
    fn game_state_roundtrip_after_moves() {
        let mut g = GameState::new();
        g.apply_move(Square::new(0, 1), Square::new(0, 3), None);
        g.apply_move(Square::new(0, 8), Square::new(0, 6), None);

        let json = serde_json::to_string(&g).unwrap();
        let back: GameState = serde_json::from_str(&json).unwrap();

        assert_eq!(g.board, back.board);
        assert_eq!(g.current_player, back.current_player);
        assert_eq!(g.moved_pawns, back.moved_pawns);
    }

    #[test]
    fn game_state_with_clock_roundtrip() {
        let g = GameState::new_with_clock(180, 2);
        let json = serde_json::to_string(&g).unwrap();
        let back: GameState = serde_json::from_str(&json).unwrap();

        let c1 = g.clock.as_ref().unwrap();
        let c2 = back.clock.as_ref().unwrap();
        assert_eq!(c1.white, c2.white);
        assert_eq!(c1.black, c2.black);
        assert_eq!(c1.increment, c2.increment);
    }

    #[test]
    fn game_state_json_keys() {
        let g = GameState::new();
        let json = serde_json::to_string(&g).unwrap();
        assert!(json.contains(r#""position":"#));
        assert!(json.contains(r#""current_player":"#));
        assert!(json.contains(r#""reserve":"#));
        assert!(json.contains(r#""reincarnating":"#));
        // history НЕ должна быть в JSON State.
        assert!(!json.contains(r#""history":"#), "history не должна сериализоваться в State");
    }

    #[test]
    fn game_state_history_not_serialized_after_moves() {
        // После 30 ходов history накапливается, но в JSON её быть не должно.
        let mut g = GameState::new();
        // Сделаем пару ходов, чтобы history стала непустой.
        g.apply_move(Square::new(0, 1), Square::new(0, 3), None);
        g.apply_move(Square::new(0, 8), Square::new(0, 6), None);
        g.apply_move(Square::new(1, 1), Square::new(1, 3), None);
        g.apply_move(Square::new(1, 8), Square::new(1, 6), None);
        assert!(!g.history.is_empty(), "history должна быть непустой");

        let json = serde_json::to_string(&g).unwrap();
        assert!(!json.contains(r#""history":"#), "history не должна быть в JSON");
        // Размер JSON должен оставаться скромным (десятки КБ), а не мегабайты.
        assert!(json.len() < 100_000, "State слишком большой: {} байт", json.len());
    }

    #[test]
    fn moved_pawns_serialized() {
        let mut g = GameState::new();
        // В 16×10 пешка в col=3 (off=3), row=1.
        g.apply_move(Square::new(3, 1), Square::new(3, 3), None);
        let json = serde_json::to_string(&g).unwrap();
        assert!(json.contains(r#""moved_pawns":"#));
        assert!(json.contains(r#""col":3"#));
        assert!(json.contains(r#""row":3"#));
    }

    #[test]
    fn reserve_serialized() {
        let g = GameState::new();
        let json = serde_json::to_string(&g).unwrap();
        assert!(json.contains("\"R2\""), "должен быть R2 в JSON: {json}");
        assert!(json.contains("\"J\""), "должен быть J в JSON: {json}");
    }

    #[test]
    fn end_reason_serialized() {
        let mut g = GameState::new();
        g.resign(Color::White);
        let json = serde_json::to_string(&g).unwrap();
        assert!(json.contains(r#""end_reason":"resign"#));
    }

    #[test]
    fn board_8x8_roundtrip() {
        let b = Board::initial_classic_8x8();
        let json = serde_json::to_string(&b).unwrap();
        let back: Board = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
        assert_eq!(b.cols(), 8);
        assert_eq!(b.rows(), 8);
        assert_eq!(back.cols(), 8);
        assert_eq!(back.rows(), 8);
    }
}
