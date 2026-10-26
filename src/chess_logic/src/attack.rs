//! Атаки, шах, мат, пат и легальность ходов.

use crate::board::Board;
use crate::moves::{get_moves, EnPassantMove};
use crate::piece::{Color, Piece, PieceType, Seat, Square};
use std::collections::{HashMap, HashSet};

pub fn is_square_attacked(board: &Board, col: i8, row: i8, by: Color) -> bool {
    let target = Square::new(col, row);
    let empty_moved: HashSet<Square> = HashSet::new();

    for (sq, piece) in board.iter_pieces() {
        if piece.color != by { continue; }

        if piece.piece_type == PieceType::Pawn {
            let (_, dr) = by.pawn_forward();
            for ddc in [-1i8, 1] {
                if sq.col + ddc == col && sq.row + dr == row {
                    return true;
                }
            }
            continue;
        }

        let moves = get_moves(board, sq, &empty_moved, None);
        if moves.contains(&target) {
            return true;
        }
    }
    false
}

pub fn is_square_attacked_by_piece(board: &Board, from: Square, target: Square) -> bool {
    let piece = match board.get(from) {
        Some(p) => p,
        None => return false,
    };

    if piece.piece_type == PieceType::Pawn {
        let (_, dr) = piece.color.pawn_forward();
        for ddc in [-1i8, 1] {
            if from.col + ddc == target.col && from.row + dr == target.row {
                return true;
            }
        }
        return false;
    }

    let mut test = board.clone();
    test.set(target, None);   // <-- КЛЮЧЕВО: убираем фигуру с target
    let empty_moved: HashSet<Square> = HashSet::new();
    let moves = get_moves(&test, from, &empty_moved, None);
    moves.contains(&target)
}

pub fn has_any_king_attacked(board: &Board, color: Color) -> bool {
    let enemy = color.opposite();
    for king_sq in board.find_all_kings(color) {
        if is_square_attacked(board, king_sq.col, king_sq.row, enemy) {
            return true;
        }
    }
    false
}

pub fn in_check(board: &Board, color: Color) -> bool {
    has_any_king_attacked(board, color)
}

pub fn filter_legal_moves(
    board: &Board,
    from: Square,
    moved_pawns: &HashSet<Square>,
    en_passant: Option<&EnPassantMove>,
) -> Vec<Square> {
    let piece = match board.get(from) {
        Some(p) => p,
        None => return Vec::new(),
    };
    let color = piece.color;
    let raw = get_moves(board, from, moved_pawns, en_passant);
    let mut legal = Vec::new();

    for &to in &raw {
        if let Some(target) = board.get(to) {
            if target.piece_type == PieceType::King && target.color != color {
                continue;
            }
        }

        let mut test = board.clone();
        let moved = test.remove(from).unwrap();

        if let Some(ep) = en_passant {
            if moved.piece_type == PieceType::Pawn && ep.pawn_color != color {
                for &crossed in &ep.crossed {
                    if to == crossed {
                        test.remove(Square::new(ep.to_col, ep.to_row));
                        break;
                    }
                }
            }
        }

        let placed = if moved.piece_type == PieceType::Pawn
        && to.row == color.pawn_promotion_row(board.rows())
        {
            Piece::new(color, PieceType::Queen)
        } else {
            moved
        };
        test.set(to, Some(placed));

        if !has_any_king_attacked(&test, color) {
            legal.push(to);
        }
    }

    legal
}

pub fn filter_legal_moves_2v2(
    board: &Board,
    from: Square,
    moved_pawns: &HashSet<Square>,
    en_passant: Option<&EnPassantMove>,
    _color: Color,
) -> Vec<Square> {
    let piece = match board.get(from) {
        Some(p) => p,
        None => return Vec::new(),
    };

    let raw = get_moves(board, from, moved_pawns, en_passant);
    let mut legal = Vec::new();

    for &to in &raw {
        if let Some(target) = board.get(to) {
            if target.piece_type.is_royal_2v2() && target.color != piece.color {
                continue;
            }
        }
        legal.push(to);
    }
    legal
}

pub fn has_in_reserve(reserve: &HashMap<Color, Vec<PieceType>>, color: Color, t: PieceType) -> bool {
    reserve.get(&color).map_or(false, |list| list.contains(&t))
}

pub fn remove_from_reserve(
    reserve: &mut HashMap<Color, Vec<PieceType>>,
    color: Color,
    t: PieceType,
) -> bool {
    if let Some(list) = reserve.get_mut(&color) {
        if let Some(pos) = list.iter().position(|&x| x == t) {
            list.remove(pos);
            return true;
        }
    }
    false
}

fn has_own_pawn_on_file(board: &Board, color: Color, col: i8) -> bool {
    for row in 0..board.rows() {
        if let Some(p) = board.get(Square::new(col, row)) {
            if p.color == color && p.piece_type == PieceType::Pawn {
                return true;
            }
        }
    }
    false
}

pub fn legal_drops(
    board: &Board,
    reserve: &HashMap<Color, Vec<PieceType>>,
    color: Color,
    piece_type: PieceType,
) -> Vec<Square> {
    if !has_in_reserve(reserve, color, piece_type) {
        return Vec::new();
    }

    let mut result = Vec::new();

    if piece_type == PieceType::Pawn {
        let start_row = color.pawn_start_row(board.rows());
        for col in 0..board.cols() {
            let sq = Square::new(col, start_row);
            if board.get(sq).is_some() { continue; }
            if has_own_pawn_on_file(board, color, col) { continue; }
            let mut test = board.clone();
            test.set(sq, Some(Piece::new(color, PieceType::Pawn)));
            if !has_any_king_attacked(&test, color) {
                result.push(sq);
            }
        }
        return result;
    }

    for col in 0..board.cols() {
        for row in 0..board.rows() {
            if !color.is_on_own_half(row, board.rows()) { continue; }
            let sq = Square::new(col, row);
            if board.get(sq).is_some() { continue; }
            let mut test = board.clone();
            test.set(sq, Some(Piece::new(color, piece_type)));
            if !has_any_king_attacked(&test, color) {
                result.push(sq);
            }
        }
    }
    result
}

pub fn has_any_legal_move(
    board: &Board,
    color: Color,
    moved_pawns: &HashSet<Square>,
    en_passant: Option<&EnPassantMove>,
    reserve: &HashMap<Color, Vec<PieceType>>,
) -> bool {
    for (sq, piece) in board.iter_pieces() {
        if piece.color != color { continue; }
        if !filter_legal_moves(board, sq, moved_pawns, en_passant).is_empty() {
            return true;
        }
    }

    if !in_check(board, color) {
        for (sq, piece) in board.iter_pieces() {
            if piece.color != color || piece.piece_type == PieceType::Pawn { continue; }
            if !crate::castle::compute_castle_moves(board, sq, color).is_empty() {
                return true;
            }
        }
    }

    if let Some(pieces) = reserve.get(&color) {
        let mut seen: HashSet<PieceType> = HashSet::new();
        for &pt in pieces {
            if !seen.insert(pt) { continue; }
            if !legal_drops(board, reserve, color, pt).is_empty() {
                return true;
            }
        }
    }

    false
}

pub fn is_checkmate(
    board: &Board,
    color: Color,
    moved_pawns: &HashSet<Square>,
    en_passant: Option<&EnPassantMove>,
    reserve: &HashMap<Color, Vec<PieceType>>,
) -> bool {
    if !in_check(board, color) { return false; }
    !has_any_legal_move(board, color, moved_pawns, en_passant, reserve)
}

pub fn is_player_royal_attacked(board: &Board, color: Color, seat: Option<Seat>) -> bool {
    let enemy = color.opposite();
    for (sq, p) in board.iter_pieces() {
        if p.color != color { continue; }
        let royal = match seat {
            Some(s) => p.piece_type.is_royal_2v2() && board.get_owner(sq) == Some(s),
            None => p.piece_type == PieceType::King,
        };
        if royal && is_square_attacked(board, sq.col, sq.row, enemy) {
            return true;
        }
    }
    false
}

pub fn has_any_move_to_save_royal(
    board: &Board,
    color: Color,
    royal_sq: Square,
    royal_pt: PieceType,
    moved_pawns: &HashSet<Square>,
    en_passant: Option<&EnPassantMove>,
) -> bool {
    debug_assert!(royal_pt.is_royal_2v2());

    for (sq, piece) in board.iter_pieces() {
        if piece.color != color { continue; }
        let legal = filter_legal_moves(board, sq, moved_pawns, en_passant);
        for to in legal {
            let mut test = board.clone();
            let moved = test.remove(sq).unwrap();
            let placed = if moved.piece_type == PieceType::Pawn
            && to.row == color.pawn_promotion_row(board.rows())
            {
                Piece::new(color, PieceType::Queen)
            } else {
                moved
            };
            test.set(to, Some(placed));

            let royal_here = test.get(royal_sq);
            let still = match royal_here {
                Some(p) if p.piece_type == royal_pt => {
                    is_square_attacked(&test, royal_sq.col, royal_sq.row, color.opposite())
                }
                _ => false,
            };
            if !still {
                return true;
            }
        }
    }
    false
}

pub fn legal_royal_return_squares(board: &Board, color: Color) -> Vec<Square> {
    let enemy = color.opposite();
    let step: i8 = if color == Color::White { 1 } else { -1 };
    let mut row = color.back_row(board.rows());
    while (0..board.rows()).contains(&row) {
        let mut row_squares = Vec::new();
        for col in 0..board.cols() {
            let sq = Square::new(col, row);
            if board.get(sq).is_some() { continue; }
            if is_square_attacked(board, col, row, enemy) { continue; }
            row_squares.push(sq);
        }
        if !row_squares.is_empty() {
            return row_squares;
        }
        row += step;
    }
    Vec::new()
}
