//! Базовые типы: цвет, тип фигуры, фигура, клетка, seat игрока.

use serde::{Deserialize, Serialize};

/// Максимальный размер доски по горизонтали (для аллокации массива).
pub const BOARD_COLS: i8 = 16;
/// Максимальный размер доски по вертикали (для аллокации массива).
pub const BOARD_ROWS: i8 = 10;

/// Размер стандартной доски 8×8.
pub const CLASSIC_SIZE: i8 = 8;

/// Устаревший алиас (для обратной совместимости).
#[deprecated(note = "используйте board.cols() / board.rows() или BOARD_COLS/BOARD_ROWS")]
pub const BOARD_SIZE: i8 = BOARD_COLS;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Color {
    #[serde(rename = "w")]
    White,
    #[serde(rename = "b")]
    Black,
}

impl Color {
    pub fn opposite(self) -> Self {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    pub fn prefix(self) -> &'static str {
        match self {
            Color::White => "w",
            Color::Black => "b",
        }
    }

    /// Начальный ряд пешек для доски `rows` (для белых — 1, для чёрных — rows-2).
    pub fn pawn_start_row(self, rows: i8) -> i8 {
        match self {
            Color::White => 1,
            Color::Black => rows - 2,
        }
    }

    /// Ряд превращения пешки для доски `rows` (для белых — rows-1, для чёрных — 0).
    pub fn pawn_promotion_row(self, rows: i8) -> i8 {
        match self {
            Color::White => rows - 1,
            Color::Black => 0,
        }
    }

    /// Направление движения пешки по вертикали (dc, dr).
    pub fn pawn_forward(self) -> (i8, i8) {
        match self {
            Color::White => (0, 1),
            Color::Black => (0, -1),
        }
    }

    /// Крайняя горизонталь (back row) для доски `rows`.
    pub fn back_row(self, rows: i8) -> i8 {
        match self {
            Color::White => 0,
            Color::Black => rows - 1,
        }
    }

    /// Половина доски «своя» для цвета (по вертикали).
    pub fn is_on_own_half(self, row: i8, rows: i8) -> bool {
        match self {
            Color::White => (0..rows / 2).contains(&row),
            Color::Black => (rows / 2..rows).contains(&row),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Seat {
    #[serde(rename = "1")]
    First,
    #[serde(rename = "2")]
    Second,
}

impl Seat {
    pub fn other(self) -> Self {
        match self {
            Seat::First => Seat::Second,
            Seat::Second => Seat::First,
        }
    }

    pub fn next(self) -> Self {
        self.other()
    }

    pub fn index(self) -> usize {
        match self {
            Seat::First => 0,
            Seat::Second => 1,
        }
    }

    pub fn all() -> [Seat; 2] {
        [Seat::First, Seat::Second]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct PlayerId {
    pub color: Color,
    pub seat: Seat,
}

impl PlayerId {
    pub fn new(color: Color, seat: Seat) -> Self {
        Self { color, seat }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum PieceType {
    #[serde(rename = "K")]
    King,
    #[serde(rename = "Q")]
    Queen,
    #[serde(rename = "R")]
    Rook,
    #[serde(rename = "B")]
    Bishop,
    #[serde(rename = "N")]
    Knight,
    #[serde(rename = "P")]
    Pawn,
    #[serde(rename = "N2")]
    Eagle,
    #[serde(rename = "R2")]
    Cheetah,
    #[serde(rename = "J")]
    Genie,
    #[serde(rename = "P2")]
    Prince,
    #[serde(rename = "P3")]
    Princess,
}

impl PieceType {
    pub fn as_str(self) -> &'static str {
        match self {
            PieceType::King => "K",
            PieceType::Queen => "Q",
            PieceType::Rook => "R",
            PieceType::Bishop => "B",
            PieceType::Knight => "N",
            PieceType::Pawn => "P",
            PieceType::Eagle => "N2",
            PieceType::Cheetah => "R2",
            PieceType::Genie => "J",
            PieceType::Prince => "P2",
            PieceType::Princess => "P3",
        }
    }

    pub fn is_royal_2v2(self) -> bool {
        matches!(self, PieceType::Prince | PieceType::Princess)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Piece {
    pub color: Color,
    #[serde(rename = "type")]
    pub piece_type: PieceType,
}

impl Piece {
    pub fn new(color: Color, piece_type: PieceType) -> Self {
        Self { color, piece_type }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Square {
    pub col: i8,
    pub row: i8,
}

impl Square {
    pub fn new(col: i8, row: i8) -> Self {
        Self { col, row }
    }

    /// Проверка на вхождение в **максимальную** доску (16×10).
    /// Для проверки на конкретной доске используйте `board.in_bounds(sq)`.
    pub fn in_bounds(self) -> bool {
        (0..BOARD_COLS).contains(&self.col) && (0..BOARD_ROWS).contains(&self.row)
    }
}

pub fn in_bounds(col: i8, row: i8) -> bool {
    (0..BOARD_COLS).contains(&col) && (0..BOARD_ROWS).contains(&row)
}
