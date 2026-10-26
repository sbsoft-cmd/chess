//! Генерация «сырых» ходов фигур.

use crate::board::Board;
use crate::piece::{Color, PieceType, Square};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EnPassantMove {
    pub from_col: i8,
    pub from_row: i8,
    pub to_col: i8,
    pub to_row: i8,
    pub pawn_color: Color,
    pub crossed: Vec<Square>,
}

pub fn get_moves(
    board: &Board,
    from: Square,
    moved_pawns: &HashSet<Square>,
    en_passant: Option<&EnPassantMove>,
) -> Vec<Square> {
    let piece = match board.get(from) {
        Some(p) => p,
        None => return Vec::new(),
    };

    match piece.piece_type {
        PieceType::Pawn => pawn_moves(board, from, piece.color, moved_pawns, en_passant),
        PieceType::Knight => knight_moves(board, from, piece.color),
        PieceType::Eagle => eagle_moves(board, from, piece.color),
        PieceType::Cheetah => cheetah_moves(board, from, piece.color),
        PieceType::Genie => genie_moves(board, from, piece.color),
        PieceType::Rook => sliding_moves(board, from, piece.color, &[(0, -1), (0, 1), (-1, 0), (1, 0)]),
        PieceType::Bishop => sliding_moves(board, from, piece.color, &[(-1, -1), (-1, 1), (1, -1), (1, 1)]),
        PieceType::Queen => sliding_moves(
            board,
            from,
            piece.color,
            &[
                (-1, -1), (-1, 1), (1, -1), (1, 1),
                                          (0, -1), (0, 1), (-1, 0), (1, 0),
            ],
        ),
        PieceType::King => king_moves(board, from, piece.color),
        PieceType::Prince | PieceType::Princess => {
            let mut moves = king_moves(board, from, piece.color);
            moves.extend(knight_moves(board, from, piece.color));
            moves
        }
    }
}

fn is_enemy(a: Color, b: Color) -> bool {
    a != b
}

fn sliding_moves(board: &Board, from: Square, color: Color, dirs: &[(i8, i8)]) -> Vec<Square> {
    let mut moves = Vec::new();
    for &(dc, dr) in dirs {
        let mut c = from.col + dc;
        let mut r = from.row + dr;
        while (0..board.cols()).contains(&c) && (0..board.rows()).contains(&r) {
            let sq = Square::new(c, r);
            match board.get(sq) {
                Some(target) => {
                    if is_enemy(color, target.color) {
                        moves.push(sq);
                    }
                    break;
                }
                None => moves.push(sq),
            }
            c += dc;
            r += dr;
        }
    }
    moves
}

fn jump_moves(board: &Board, from: Square, color: Color, deltas: &[(i8, i8)]) -> Vec<Square> {
    let mut moves = Vec::new();
    for &(dc, dr) in deltas {
        let c = from.col + dc;
        let r = from.row + dr;
        if !(0..board.cols()).contains(&c) || !(0..board.rows()).contains(&r) {
            continue;
        }
        let sq = Square::new(c, r);
        match board.get(sq) {
            Some(target) => {
                if is_enemy(color, target.color) {
                    moves.push(sq);
                }
            }
            None => moves.push(sq),
        }
    }
    moves
}

fn knight_moves(board: &Board, from: Square, color: Color) -> Vec<Square> {
    const DELTAS: [(i8, i8); 8] = [
        (1, 2), (2, 1), (-1, 2), (-2, 1),
        (1, -2), (2, -1), (-1, -2), (-2, -1),
    ];
    jump_moves(board, from, color, &DELTAS)
}

fn eagle_moves(board: &Board, from: Square, color: Color) -> Vec<Square> {
    const DELTAS: [(i8, i8); 8] = [
        (1, 3), (3, 1), (-1, 3), (-3, 1),
        (1, -3), (3, -1), (-1, -3), (-3, -1),
    ];
    jump_moves(board, from, color, &DELTAS)
}

fn king_moves(board: &Board, from: Square, color: Color) -> Vec<Square> {
    let mut moves = Vec::new();
    for dc in -1..=1 {
        for dr in -1..=1 {
            if dc == 0 && dr == 0 { continue; }
            let c = from.col + dc;
            let r = from.row + dr;
            if !(0..board.cols()).contains(&c) || !(0..board.rows()).contains(&r) { continue; }
            let sq = Square::new(c, r);
            match board.get(sq) {
                Some(target) => {
                    if is_enemy(color, target.color) {
                        moves.push(sq);
                    }
                }
                None => moves.push(sq),
            }
        }
    }
    moves
}

fn cheetah_moves(board: &Board, from: Square, color: Color) -> Vec<Square> {
    jump_over_one(board, from, color, &[(0, -1), (0, 1), (-1, 0), (1, 0)])
}

fn genie_moves(board: &Board, from: Square, color: Color) -> Vec<Square> {
    jump_over_one(
        board,
        from,
        color,
        &[
            (0, -1), (0, 1), (-1, 0), (1, 0),
                  (-1, -1), (-1, 1), (1, -1), (1, 1),
        ],
    )
}

fn jump_over_one(board: &Board, from: Square, color: Color, dirs: &[(i8, i8)]) -> Vec<Square> {
    let mut moves = Vec::new();
    for &(dc, dr) in dirs {
        let mut nc = from.col + dc;
        let mut nr = from.row + dr;
        let mut first_blocker = false;
        let mut jumped = false;

        while (0..board.cols()).contains(&nc) && (0..board.rows()).contains(&nr) {
            let sq = Square::new(nc, nr);
            match board.get(sq) {
                Some(target) => {
                    if !first_blocker {
                        first_blocker = true;
                        if is_enemy(color, target.color) {
                            moves.push(sq);
                        }
                        let jc = nc + dc;
                        let jr = nr + dr;
                        if (0..board.cols()).contains(&jc) && (0..board.rows()).contains(&jr) {
                            let jump_sq = Square::new(jc, jr);
                            if board.get(jump_sq).is_none() {
                                jumped = true;
                            }
                        }
                        if !jumped { break; }
                    } else {
                        break;
                    }
                }
                None => moves.push(sq),
            }
            nc += dc;
            nr += dr;
        }
    }
    moves
}

fn pawn_moves(
    board: &Board,
    from: Square,
    color: Color,
    moved_pawns: &HashSet<Square>,
    en_passant: Option<&EnPassantMove>,
) -> Vec<Square> {
    let mut moves = Vec::new();
    let (_, dr) = color.pawn_forward();

    let on_start = !moved_pawns.contains(&from);
    // Для 8×8 (классика) пешка ходит на 2 с начального и на 1 после.
    // Для 10×10 (Duel) — на 3 с начального и на 2 после.
    let max_steps: i8 = if board.rows() == 8 {
        if on_start { 2 } else { 1 }
    } else {
        if on_start { 3 } else { 2 }
    };

    for step in 1..=max_steps {
        let c = from.col;
        let r = from.row + dr * step;
        if !(0..board.rows()).contains(&r) { break; }
        let sq = Square::new(c, r);
        if board.get(sq).is_some() { break; }
        moves.push(sq);
    }

    for ddc in [-1, 1] {
        let c = from.col + ddc;
        let r = from.row + dr;
        if !(0..board.cols()).contains(&c) || !(0..board.rows()).contains(&r) { continue; }
        let sq = Square::new(c, r);
        if let Some(target) = board.get(sq) {
            if is_enemy(color, target.color) {
                moves.push(sq);
            }
        }
    }

    if let Some(ep) = en_passant {
        if ep.pawn_color != color {
            for &crossed in &ep.crossed {
                let our_row = crossed.row - dr;
                if from.row == our_row {
                    let dc = from.col - crossed.col;
                    if dc == 1 || dc == -1 {
                        moves.push(crossed);
                    }
                }
            }
        }
    }

    moves
}
