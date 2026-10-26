//! Случайная расстановка фигур (Freestyle).

use crate::board::Board;
use crate::piece::{Color, Piece, PieceType, Square, BOARD_SIZE};
use rand::seq::SliceRandom;

/// Случайная расстановка 1-го ряда белых.
///
/// Ограничения:
///   - слоны (Bishop) на разных цветах клеток (разная чётность столбцов);
///   - Орлы (Eagle) на разных цветах клеток.
pub fn random_back_rank() -> [PieceType; 10] {
    let mut pieces: [PieceType; 10] = [
        PieceType::Rook,
        PieceType::Rook,
        PieceType::Eagle,
        PieceType::Eagle,
        PieceType::Knight,
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Bishop,
        PieceType::Queen,
        PieceType::King,
    ];

    let mut rng = rand::thread_rng();
    loop {
        pieces.shuffle(&mut rng);

        if !pieces_on_different_colors(&pieces, PieceType::Bishop) {
            continue;
        }
        if !pieces_on_different_colors(&pieces, PieceType::Eagle) {
            continue;
        }
        return pieces;
    }
}

/// Проверить, что обе фигуры данного типа стоят на столбцах
/// разной чётности (разные цвета клеток).
fn pieces_on_different_colors(pieces: &[PieceType; 10], pt: PieceType) -> bool {
    let positions: Vec<usize> = pieces
    .iter()
    .enumerate()
    .filter_map(|(i, p)| if *p == pt { Some(i) } else { None })
    .collect();
    if positions.len() != 2 {
        return false;
    }
    positions[0] % 2 != positions[1] % 2
}

/// Применить случайную расстановку: белые на ряд 0, чёрные — зеркально на ряд 9.
///
/// Фигуры на других рядах (пешки, запас) не трогаются.
pub fn apply_random_position(board: &mut Board) {
    let back_rank = random_back_rank();
    let cols = board.cols();
    let rows = board.rows();

    // Для 16×10 (Duel) оставляем 10 фигур по центру (off = 3).
    // Для 8×8 — 8 фигур (random_back_rank возвращает 10, обрезаем до 8).
    let (off, count) = if cols == 16 {
        (3, 10)
    } else {
        (0, cols as usize)
    };

    for col in 0..cols {
        board.set(Square::new(col, 0), None);
        board.set(Square::new(col, rows - 1), None);
    }

    for i in 0..count.min(back_rank.len()) {
        let col = off + i as i8;
        board.set(Square::new(col, 0), Some(Piece::new(Color::White, back_rank[i])));
        board.set(Square::new(col, rows - 1), Some(Piece::new(Color::Black, back_rank[i])));
    }
}

/// Случайная расстановка back rank для 8×8 (шахматы Фишера).
///
/// Ограничения:
///   - Король между двумя Ладьями;
///   - Слоны на разных цветах клеток (разная чётность столбцов);
///   - Остальные фигуры (Q, N, N) — на оставшихся клетках.
pub fn random_back_rank_8x8() -> [PieceType; 8] {
    use rand::seq::SliceRandom;

    let mut rng = rand::thread_rng();
    let mut pieces: [PieceType; 8] = [
        PieceType::Rook,
        PieceType::Rook,
        PieceType::King,
        PieceType::Queen,
        PieceType::Bishop,
        PieceType::Bishop,
        PieceType::Knight,
        PieceType::Knight,
    ];

    loop {
        pieces.shuffle(&mut rng);

        // Слоны на разных цветах клеток.
        let bishops: Vec<usize> = pieces
        .iter()
        .enumerate()
        .filter_map(|(i, p)| if *p == PieceType::Bishop { Some(i) } else { None })
        .collect();
        if bishops.len() != 2 || bishops[0] % 2 == bishops[1] % 2 {
            continue;
        }

        // Король между двумя Ладьями.
        let king_pos = pieces
        .iter()
        .position(|p| *p == PieceType::King)
        .unwrap();
        let rooks: Vec<usize> = pieces
        .iter()
        .enumerate()
        .filter_map(|(i, p)| if *p == PieceType::Rook { Some(i) } else { None })
        .collect();
        if rooks.len() != 2 {
            continue;
        }
        if !(rooks[0] < king_pos && king_pos < rooks[1]) {
            continue;
        }

        return pieces;
    }
}

/// Применить случайную расстановку 8×8: белые на ряд 0, чёрные зеркально на ряд 7.
/// Пешки остаются на своих местах (ряд 1 и 6).
pub fn apply_random_position_8x8(board: &mut Board) {
    let back_rank = random_back_rank_8x8();
    let cols = board.cols();
    let rows = board.rows();
    debug_assert_eq!(cols, 8);
    debug_assert_eq!(rows, 8);

    for col in 0..cols {
        board.set(Square::new(col, 0), Some(Piece::new(Color::White, back_rank[col as usize])));
        board.set(Square::new(col, rows - 1), Some(Piece::new(Color::Black, back_rank[col as usize])));
    }
}

// ============================================================
// Тесты
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn random_back_rank_has_correct_pieces() {
        let rank = random_back_rank();
        let set: HashSet<PieceType> = rank.iter().copied().collect();
        assert!(set.contains(&PieceType::King));
        assert!(set.contains(&PieceType::Queen));
        assert!(set.contains(&PieceType::Rook));
        assert!(set.contains(&PieceType::Bishop));
        assert!(set.contains(&PieceType::Knight));
        assert!(set.contains(&PieceType::Eagle));
        assert_eq!(rank.len(), 10);
    }

    #[test]
    fn random_back_rank_bishops_on_different_colors() {
        for _ in 0..50 {
            let rank = random_back_rank();
            let bishops: Vec<usize> = rank
            .iter()
            .enumerate()
            .filter_map(|(i, p)| if *p == PieceType::Bishop { Some(i) } else { None })
            .collect();
            assert_eq!(bishops.len(), 2);
            assert_ne!(
                bishops[0] % 2,
                bishops[1] % 2,
                "слоны должны быть на разных цветах клеток"
            );
        }
    }

    #[test]
    fn random_back_rank_eagles_on_different_colors() {
        for _ in 0..50 {
            let rank = random_back_rank();
            let eagles: Vec<usize> = rank
            .iter()
            .enumerate()
            .filter_map(|(i, p)| if *p == PieceType::Eagle { Some(i) } else { None })
            .collect();
            assert_eq!(eagles.len(), 2);
            assert_ne!(
                eagles[0] % 2,
                eagles[1] % 2,
                "Орлы должны быть на разных цветах клеток"
            );
        }
    }

    #[test]
    fn apply_random_position_keeps_pawns() {
        let mut b = Board::initial(); // 16×10
        for col in 3..13 {
            assert!(b.get(Square::new(col, 1)).is_some());
        }
        apply_random_position(&mut b);
        for col in 3..13 {
            assert_eq!(
                b.get(Square::new(col, 1)).unwrap().piece_type,
                       PieceType::Pawn
            );
            assert_eq!(
                b.get(Square::new(col, b.rows() - 2)).unwrap().piece_type,
                       PieceType::Pawn
            );
        }
    }

    #[test]
    fn apply_random_position_fills_back_rows() {
        let mut b = Board::initial();
        apply_random_position(&mut b);
        for col in 3..13 {
            assert!(b.get(Square::new(col, 0)).is_some(), "ряд 0 заполнен в col={col}");
            assert!(b.get(Square::new(col, b.rows() - 1)).is_some(), "ряд 9 заполнен в col={col}");
        }
    }

    #[test]
    fn apply_random_position_mirrors_white_black() {
        let mut b = Board::initial();
        apply_random_position(&mut b);
        for col in 3..13 {
            let white = b.get(Square::new(col, 0)).unwrap();
            let black = b.get(Square::new(col, b.rows() - 1)).unwrap();
            assert_eq!(white.piece_type, black.piece_type, "зеркальность по col");
            assert_eq!(white.color, Color::White);
            assert_eq!(black.color, Color::Black);
        }
    }

}

#[cfg(test)]
mod tests_8x8 {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn random_back_rank_8x8_has_correct_pieces() {
        for _ in 0..200 {
            let rank = random_back_rank_8x8();
            let set: HashSet<PieceType> = rank.iter().copied().collect();
            assert!(set.contains(&PieceType::King));
            assert!(set.contains(&PieceType::Queen));
            assert_eq!(rank.iter().filter(|p| **p == PieceType::Rook).count(), 2);
            assert_eq!(rank.iter().filter(|p| **p == PieceType::Bishop).count(), 2);
            assert_eq!(rank.iter().filter(|p| **p == PieceType::Knight).count(), 2);
            assert_eq!(rank.len(), 8);
        }
    }

    #[test]
    fn random_back_rank_8x8_bishops_on_different_colors() {
        for _ in 0..200 {
            let rank = random_back_rank_8x8();
            let bishops: Vec<usize> = rank
            .iter()
            .enumerate()
            .filter_map(|(i, p)| if *p == PieceType::Bishop { Some(i) } else { None })
            .collect();
            assert_ne!(bishops[0] % 2, bishops[1] % 2);
        }
    }

    #[test]
    fn random_back_rank_8x8_king_between_rooks() {
        for _ in 0..200 {
            let rank = random_back_rank_8x8();
            let king = rank.iter().position(|p| *p == PieceType::King).unwrap();
            let rooks: Vec<usize> = rank
            .iter()
            .enumerate()
            .filter_map(|(i, p)| if *p == PieceType::Rook { Some(i) } else { None })
            .collect();
            assert!(rooks[0] < king && king < rooks[1]);
        }
    }

    #[test]
    fn apply_random_position_8x8_keeps_pawns() {
        let mut b = Board::initial_classic_8x8();
        for col in 0..8 {
            assert!(b.get(Square::new(col, 1)).is_some());
            assert!(b.get(Square::new(col, 6)).is_some());
        }
        apply_random_position_8x8(&mut b);
        for col in 0..8 {
            assert_eq!(b.get(Square::new(col, 1)).unwrap().piece_type, PieceType::Pawn);
            assert_eq!(b.get(Square::new(col, 6)).unwrap().piece_type, PieceType::Pawn);
        }
    }
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
        b.set(Square::new(1, 0), Some(Piece::new(Color::White, PieceType::King))); // b1
        b.set(Square::new(0, 0), Some(Piece::new(Color::White, PieceType::Rook))); // a1

        assert!(can_castle(&b, Square::new(1, 0), Square::new(0, 0), Color::White));
        apply_castle_classic(&mut b, Square::new(1, 0), Square::new(0, 0));

        // Король на c1, ладья на d1.
        assert_eq!(b.get(Square::new(2, 0)).unwrap().piece_type, PieceType::King);
        assert_eq!(b.get(Square::new(3, 0)).unwrap().piece_type, PieceType::Rook);
    }

    #[test]
    fn fischer_castle_king_on_g1_rook_on_h1() {
        let mut b = empty_board();
        b.set(Square::new(6, 0), Some(Piece::new(Color::White, PieceType::King))); // g1
        b.set(Square::new(7, 0), Some(Piece::new(Color::White, PieceType::Rook))); // h1

        assert!(can_castle(&b, Square::new(6, 0), Square::new(7, 0), Color::White));
        apply_castle_classic(&mut b, Square::new(6, 0), Square::new(7, 0));

        // Король остаётся на g1, ладья на f1.
        assert_eq!(b.get(Square::new(6, 0)).unwrap().piece_type, PieceType::King);
        assert_eq!(b.get(Square::new(5, 0)).unwrap().piece_type, PieceType::Rook);
    }

    #[test]
    fn fischer_castle_king_on_f1_rook_on_g1_swap() {
        let mut b = empty_board();
        b.set(Square::new(5, 0), Some(Piece::new(Color::White, PieceType::King))); // f1
        b.set(Square::new(6, 0), Some(Piece::new(Color::White, PieceType::Rook))); // g1

        assert!(can_castle(&b, Square::new(5, 0), Square::new(6, 0), Color::White));
        apply_castle_classic(&mut b, Square::new(5, 0), Square::new(6, 0));

        // Король на g1, ладья на f1.
        assert_eq!(b.get(Square::new(6, 0)).unwrap().piece_type, PieceType::King);
        assert_eq!(b.get(Square::new(5, 0)).unwrap().piece_type, PieceType::Rook);
    }
}
