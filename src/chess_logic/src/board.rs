//! Доска с динамическим размером (8×8, 10×10, 16×10).

use crate::piece::{
    Color, Piece, PieceType, Seat, Square, BOARD_COLS, BOARD_ROWS, CLASSIC_SIZE,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    cells:  [[Option<Piece>; BOARD_ROWS as usize]; BOARD_COLS as usize],
    owners: [[Option<Seat>;  BOARD_ROWS as usize]; BOARD_COLS as usize],
    cols: i8,
    rows: i8,
}

impl Board {
    /// Пустая доска максимального размера (16×10).
    pub fn empty() -> Self {
        Self {
            cells:  [[None; BOARD_ROWS as usize]; BOARD_COLS as usize],
            owners: [[None; BOARD_ROWS as usize]; BOARD_COLS as usize],
            cols: BOARD_COLS,
            rows: BOARD_ROWS,
        }
    }

    /// Пустая доска заданного размера.
    pub fn empty_with_size(cols: i8, rows: i8) -> Self {
        let mut b = Self::empty();
        b.cols = cols;
        b.rows = rows;
        //println!("[empty_with_size] cols={}, rows={}", b.cols, b.rows);
        b
    }

    pub fn cols(&self) -> i8 { self.cols }
    pub fn rows(&self) -> i8 { self.rows }

    /// Проверка, что клетка внутри этой доски.
    pub fn in_bounds(&self, sq: Square) -> bool {
        (0..self.cols).contains(&sq.col) && (0..self.rows).contains(&sq.row)
    }

    /// Классическая расстановка 8×8.
    pub fn initial_classic_8x8() -> Self {
        let mut b = Self::empty_with_size(8, 8);
        println!("[initial_classic_8x8] after empty_with_size: cols={}, rows={}", b.cols(), b.rows());

        let back: [PieceType; 8] = [
            PieceType::Rook,
            PieceType::Knight,
            PieceType::Bishop,
            PieceType::Queen,
            PieceType::King,
            PieceType::Bishop,
            PieceType::Knight,
            PieceType::Rook,
        ];

        for (col, pt) in back.iter().enumerate() {
            b.set(Square::new(col as i8, 0), Some(Piece::new(Color::White, *pt)));
            b.set(Square::new(col as i8, 7), Some(Piece::new(Color::Black, *pt)));
        }
        for col in 0..CLASSIC_SIZE {
            b.set(Square::new(col, 1), Some(Piece::new(Color::White, PieceType::Pawn)));
            b.set(Square::new(col, 6), Some(Piece::new(Color::Black, PieceType::Pawn)));
        }
        b
    }

    /// Старая расстановка 10×10 (duel), вписанная в 16 столбцов.
    pub fn initial() -> Self {
        let mut b = Self::empty_with_size(10, 10); // <-- 10×10!
        let off = 0; // <-- без сдвига
        // ...
        let white_back: [PieceType; 10] = [
            PieceType::Eagle,
            PieceType::Rook,
            PieceType::Knight,
            PieceType::Bishop,
            PieceType::Queen,
            PieceType::King,
            PieceType::Bishop,
            PieceType::Knight,
            PieceType::Rook,
            PieceType::Eagle,
        ];
        for (i, pt) in white_back.iter().enumerate() {
            b.set(Square::new(off + i as i8, 0), Some(Piece::new(Color::White, *pt)));
        }
        for i in 0..10 {
            b.set(Square::new(off + i, 1), Some(Piece::new(Color::White, PieceType::Pawn)));
        }
        for i in 0..10 {
            let pt = white_back[i as usize];
            b.set(Square::new(off + i, BOARD_ROWS - 1), Some(Piece::new(Color::Black, pt)));
            b.set(Square::new(off + i, BOARD_ROWS - 2), Some(Piece::new(Color::Black, PieceType::Pawn)));
        }
        b
    }

    /// Расстановка 2-на-2 на 16×10.
    pub fn initial_team_2v2() -> Self {
        let mut b = Self::empty_with_size(16, 10);

        // Первый игрок: R N B E C J Pr Q (Принцесса на col 6, Ферзь на col 7).
        let first_white: [PieceType; 8] = [
            PieceType::Rook,
            PieceType::Knight,
            PieceType::Bishop,
            PieceType::Eagle,
            PieceType::Cheetah,
            PieceType::Genie,
            PieceType::Princess,
            PieceType::Queen,
        ];
        // Второй игрок: K P2 J C E B N R (Король на col 8, Принц на col 9).
        let second_white: [PieceType; 8] = [
            PieceType::King,
            PieceType::Prince,
            PieceType::Genie,
            PieceType::Cheetah,
            PieceType::Eagle,
            PieceType::Bishop,
            PieceType::Knight,
            PieceType::Rook,
        ];

        for (col, pt) in first_white.iter().enumerate() {
            b.set_with_owner(
                Square::new(col as i8, 0),
                             Some(Piece::new(Color::White, *pt)),
                             Some(Seat::First),
            );
        }
        for (i, pt) in second_white.iter().enumerate() {
            b.set_with_owner(
                Square::new(8 + i as i8, 0),
                             Some(Piece::new(Color::White, *pt)),
                             Some(Seat::Second),
            );
        }
        for col in 0..BOARD_COLS {
            let seat = if col < 8 { Seat::First } else { Seat::Second };
            b.set_with_owner(
                Square::new(col, 1),
                             Some(Piece::new(Color::White, PieceType::Pawn)),
                             Some(seat),
            );
        }

        for (col, pt) in first_white.iter().enumerate() {
            b.set_with_owner(
                Square::new(col as i8, BOARD_ROWS - 1),
                             Some(Piece::new(Color::Black, *pt)),
                             Some(Seat::First),
            );
        }
        for (i, pt) in second_white.iter().enumerate() {
            b.set_with_owner(
                Square::new(8 + i as i8, BOARD_ROWS - 1),
                             Some(Piece::new(Color::Black, *pt)),
                             Some(Seat::Second),
            );
        }
        for col in 0..BOARD_COLS {
            let seat = if col < 8 { Seat::First } else { Seat::Second };
            b.set_with_owner(
                Square::new(col, BOARD_ROWS - 2),
                             Some(Piece::new(Color::Black, PieceType::Pawn)),
                             Some(seat),
            );
        }

        b
    }

    pub fn get(&self, sq: Square) -> Option<Piece> {
        if !self.in_bounds(sq) { return None; }
        self.cells[sq.col as usize][sq.row as usize]
    }

    pub fn set(&mut self, sq: Square, piece: Option<Piece>) {
        if !self.in_bounds(sq) { return; }
        self.cells[sq.col as usize][sq.row as usize] = piece;
    }

    pub fn get_owner(&self, sq: Square) -> Option<Seat> {
        if !self.in_bounds(sq) { return None; }
        self.owners[sq.col as usize][sq.row as usize]
    }

    pub fn set_owner(&mut self, sq: Square, seat: Option<Seat>) {
        if !self.in_bounds(sq) { return; }
        self.owners[sq.col as usize][sq.row as usize] = seat;
    }

    pub fn set_with_owner(&mut self, sq: Square, piece: Option<Piece>, seat: Option<Seat>) {
        self.set(sq, piece);
        self.set_owner(sq, seat);
    }

    pub fn remove(&mut self, sq: Square) -> Option<Piece> {
        let old = self.get(sq);
        self.set(sq, None);
        self.set_owner(sq, None);
        old
    }

    pub fn remove_all_of_player(&mut self, color: Color, seat: Seat) -> Vec<Square> {
        let mut removed = Vec::new();
        for col in 0..self.cols {
            for row in 0..self.rows {
                let sq = Square::new(col, row);
                if self.get_owner(sq) == Some(seat) {
                    if let Some(p) = self.get(sq) {
                        if p.color == color {
                            self.set(sq, None);
                            self.set_owner(sq, None);
                            removed.push(sq);
                        }
                    }
                }
            }
        }
        removed
    }

    pub fn find_player_royal(&self, color: Color, seat: Seat, pt: PieceType) -> Option<Square> {
        debug_assert!(pt.is_royal_2v2());
        for (sq, p) in self.iter_pieces() {
            if p.color == color && p.piece_type == pt && self.get_owner(sq) == Some(seat) {
                return Some(sq);
            }
        }
        None
    }

    pub fn iter_pieces(&self) -> impl Iterator<Item = (Square, Piece)> + '_ {
        self.cells.iter().enumerate().flat_map(move |(col, column)| {
            let rows = self.rows;
            column.iter().take(rows as usize).enumerate().filter_map(move |(row, cell)| {
                cell.map(|p| (Square::new(col as i8, row as i8), p))
            })
        })
    }

    pub fn find_king(&self, color: Color) -> Option<Square> {
        self.iter_pieces()
        .find(|(_, p)| p.color == color && p.piece_type == PieceType::King)
        .map(|(sq, _)| sq)
    }

    pub fn find_all_kings(&self, color: Color) -> Vec<Square> {
        self.iter_pieces()
        .filter(|(_, p)| p.color == color && p.piece_type == PieceType::King)
        .map(|(sq, _)| sq)
        .collect()
    }

    pub fn count(&self) -> usize {
        self.iter_pieces().count()
    }
}
