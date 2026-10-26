// //! Рокировки.
//!
//! 8×8 / 8×8 free — только Король + Ладья, правила Фишера.
//! 10×10 / 10×10 free / 16×10 — «рокировка» = обмен местами любых двух
//! своих фигур, если хотя бы одна «атакует» другую по своим правилам
//! (для линейных — путь свободен; конь/орёл/король прыгают).

use crate::attack::{is_square_attacked, is_square_attacked_by_piece};
use crate::board::Board;
use crate::piece::{Color, PieceType, Square};

/// Безопасна ли классическая рокировка короля с ладьёй (8×8, Фишер).
pub fn is_king_castle_safe(board: &Board, king: Square, partner: Square, color: Color) -> bool {
    let enemy = color.opposite();
    let is_royal = board.get(king).map_or(false, |p| p.piece_type.is_royal_2v2());
    if is_royal {
        return true;
    }

    if is_square_attacked(board, king.col, king.row, enemy) {
        return false;
    }

    let dir = if partner.col > king.col { 1 } else { -1 };
    for step in 1..=2 {
        let col = king.col + step * dir;
        if is_square_attacked(board, col, king.row, enemy) {
            return false;
        }
    }
    true
}

/// Можно ли рокироваться между двумя клетками.
pub fn can_castle(board: &Board, a: Square, c: Square, color: Color) -> bool {
    let pa = match board.get(a) {
        Some(p) => p,
        None => return false,
    };
    let pc = match board.get(c) {
        Some(p) => p,
        None => return false,
    };
    if pa.color != color || pc.color != color {
        return false;
    }
    if pa.piece_type == PieceType::Pawn || pc.piece_type == PieceType::Pawn {
        return false;
    }

    // ============================================================
    // 10×10 / 16×10: рокировка-обмен любых двух своих фигур,
    // если хотя бы одна «атакует» другую по своим правилам.
    // Для ЛЮБОЙ пары путь между ними должен быть свободен —
    // это запрещает рокировку «через третью фигуру» (в т.ч. для Джина).
    // ============================================================
    if board.cols() >= 10 {
        if !is_path_clear(board, a, c) {
            return false;
        }

        let a_attacks_c = is_square_attacked_by_piece(board, a, c);
        let c_attacks_a = is_square_attacked_by_piece(board, c, a);
        return a_attacks_c || c_attacks_a;
    }
    // ============================================================
    // 8×8 / 8×8 free: только Король + Ладья (Фишер).
    // ============================================================
    let (king, partner, is_king) = if pa.piece_type == PieceType::King {
        (a, c, true)
    } else if pc.piece_type == PieceType::King {
        (c, a, true)
    } else {
        (a, c, false)
    };

    if !is_king {
        return false;
    }

    if king.row != color.back_row(board.rows()) {
        return false;
    }
    if partner.row != color.back_row(board.rows()) {
        return false;
    }

    let partner_piece = match board.get(partner) {
        Some(p) => p,
        None => return false,
    };
    if partner_piece.piece_type != PieceType::Rook {
        return false;
    }

    let back = color.back_row(board.rows());
    let king_target_col = if partner.col > king.col { 6 } else { 2 };
    let rook_target_col = if partner.col > king.col { 5 } else { 3 };

    let king_target = Square::new(king_target_col, back);
    let rook_target = Square::new(rook_target_col, back);

    if !is_path_clear_ignore(board, king, king_target, &[king, partner]) {
        return false;
    }
    if !is_path_clear_ignore(board, partner, rook_target, &[king, partner]) {
        return false;
    }

    let enemy = color.opposite();
    if is_square_attacked(board, king.col, king.row, enemy) {
        return false;
    }

    let step = if king_target_col > king.col {
        1
    } else if king_target_col < king.col {
        -1
    } else {
        0
    };
    let mut cur_col = king.col;
    while cur_col != king_target_col {
        cur_col += step;
        if is_square_attacked(board, cur_col, back, enemy) {
            return false;
        }
    }

    true
}

/// Классическая рокировка Король + Ладья для 8×8 (Фишер).
/// Король → g1/c1, Ладья → f1/d1, независимо от исходных позиций.
pub fn apply_castle_classic(board: &mut Board, king: Square, rook: Square) {
    let color = board.get(king).map(|p| p.color).unwrap_or(Color::White);
    let back = color.back_row(board.rows());
    let kingside = rook.col > king.col;

    let king_target = Square::new(if kingside { 6 } else { 2 }, back);
    let rook_target = Square::new(if kingside { 5 } else { 3 }, back);

    let k = board.remove(king);
    let r = board.remove(rook);

    if let Some(k) = k {
        board.set(king_target, Some(k));
    }
    if let Some(r) = r {
        board.set(rook_target, Some(r));
    }
}

/// Рокировка-обмен местами (10×10 / 16×10).
pub fn apply_castle_swap(board: &mut Board, a: Square, c: Square) {
    let pa = board.get(a);
    let pc = board.get(c);
    board.set(a, pc);
    board.set(c, pa);
}

pub fn compute_castle_moves(board: &Board, from: Square, color: Color) -> Vec<Square> {
    let piece = match board.get(from) {
        Some(p) => p,
        None => {
            println!("[compute_castle_moves] нет фигуры на {:?}", from);
            return Vec::new();
        }
    };
    //println!(
    //    "[compute_castle_moves] from={:?}, piece={:?}, color={:?}",
    //    from, piece, color
    //);
    if piece.color != color || piece.piece_type == PieceType::Pawn {
        return Vec::new();
    }

    let mut res = Vec::new();
    for (sq, target) in board.iter_pieces() {
        if sq == from {
            continue;
        }
        if target.color != color {
            continue;
        }
        if can_castle(board, from, sq, color) {
            res.push(sq);
        }
    }
    //println!("[compute_castle_moves] result={:?}", res);
    res
}

/// Универсальная рокировка для 10×10 / 16×10 / 8×8:
/// если это Король + Ладья на 8×8 — Фишер, иначе — обмен местами.
pub fn apply_castle(board: &mut Board, a: Square, c: Square) {
    let a_piece = board.get(a);
    let c_piece = board.get(c);

    // 8×8: Король + Ладья → классическая рокировка (Фишер).
    let is_8x8 = board.cols() == 8;
    let a_is_king = a_piece.map_or(false, |p| p.piece_type == PieceType::King);
    let c_is_king = c_piece.map_or(false, |p| p.piece_type == PieceType::King);
    let a_is_rook = a_piece.map_or(false, |p| p.piece_type == PieceType::Rook);
    let c_is_rook = c_piece.map_or(false, |p| p.piece_type == PieceType::Rook);
    let king_rook = (a_is_king && c_is_rook) || (c_is_king && a_is_rook);

    if is_8x8 && king_rook {
        let (king_sq, rook_sq) = if a_is_king { (a, c) } else { (c, a) };
        apply_castle_classic(board, king_sq, rook_sq);
    } else {
        apply_castle_swap(board, a, c);
    }
}

fn is_path_clear(board: &Board, a: Square, c: Square) -> bool {
    if a == c {
        return true;
    }
    let dc = (c.col - a.col).signum();
    let dr = (c.row - a.row).signum();
    if dc != 0 && dr != 0 && (c.col - a.col).abs() != (c.row - a.row).abs() {
        return false;
    }
    let mut col = a.col + dc;
    let mut row = a.row + dr;
    while (col, row) != (c.col, c.row) {
        if board.get(Square::new(col, row)).is_some() {
            return false;
        }
        col += dc;
        row += dr;
    }
    true
}

/// Свободен ли путь между `a` и `b`, игнорируя клетки из `ignore`.
fn is_path_clear_ignore(board: &Board, a: Square, b: Square, ignore: &[Square]) -> bool {
    if a == b {
        return true;
    }
    let dc = (b.col - a.col).signum();
    let dr = (b.row - a.row).signum();
    if dc != 0 && dr != 0 && (b.col - a.col).abs() != (b.row - a.row).abs() {
        return false;
    }
    let mut c = a.col + dc;
    let mut r = a.row + dr;
    while (c, r) != (b.col, b.row) {
        let sq = Square::new(c, r);
        if !ignore.contains(&sq) && board.get(sq).is_some() {
            return false;
        }
        c += dc;
        r += dr;
    }
    true
}

#[cfg(test)]
mod tests_fischer {
    use super::*;
    use crate::board::Board;
    use crate::piece::{Color, Piece, PieceType, Square};

    fn empty_board() -> Board {
        Board::empty_with_size(8, 8)
    }

    #[test]
    fn fischer_castle_king_on_b1_rook_on_a1() {
        let mut b = empty_board();
        b.set(Square::new(1, 0), Some(Piece::new(Color::White, PieceType::King)));
        b.set(Square::new(0, 0), Some(Piece::new(Color::White, PieceType::Rook)));

        assert!(can_castle(&b, Square::new(1, 0), Square::new(0, 0), Color::White));
        apply_castle_classic(&mut b, Square::new(1, 0), Square::new(0, 0));

        assert_eq!(b.get(Square::new(2, 0)).unwrap().piece_type, PieceType::King);
        assert_eq!(b.get(Square::new(3, 0)).unwrap().piece_type, PieceType::Rook);
    }

    #[test]
    fn fischer_castle_king_on_g1_rook_on_h1() {
        let mut b = empty_board();
        b.set(Square::new(6, 0), Some(Piece::new(Color::White, PieceType::King)));
        b.set(Square::new(7, 0), Some(Piece::new(Color::White, PieceType::Rook)));

        assert!(can_castle(&b, Square::new(6, 0), Square::new(7, 0), Color::White));
        apply_castle_classic(&mut b, Square::new(6, 0), Square::new(7, 0));

        assert_eq!(b.get(Square::new(6, 0)).unwrap().piece_type, PieceType::King);
        assert_eq!(b.get(Square::new(5, 0)).unwrap().piece_type, PieceType::Rook);
    }

    #[test]
    fn fischer_castle_king_on_f1_rook_on_g1_swap() {
        let mut b = empty_board();
        b.set(Square::new(5, 0), Some(Piece::new(Color::White, PieceType::King)));
        b.set(Square::new(6, 0), Some(Piece::new(Color::White, PieceType::Rook)));

        assert!(can_castle(&b, Square::new(5, 0), Square::new(6, 0), Color::White));
        apply_castle_classic(&mut b, Square::new(5, 0), Square::new(6, 0));

        assert_eq!(b.get(Square::new(6, 0)).unwrap().piece_type, PieceType::King);
        assert_eq!(b.get(Square::new(5, 0)).unwrap().piece_type, PieceType::Rook);
    }
}
