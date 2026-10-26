//! Реинкарнация пешек: срубленная пешка возвращается через N ходов.

use crate::piece::{Color, PieceType};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Сколько своих ходов пешка ждёт в очереди.
pub const REINCARNATION_TURNS: i32 = 3;

/// Запись в очереди реинкарнации.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReincarnatingPawn {
    pub turns_left: i32,
    pub ready: bool,
}


impl ReincarnatingPawn {
    pub fn new() -> Self {
        Self {
            turns_left: REINCARNATION_TURNS,
            ready: false,
        }
    }
}

/// Очередь реинкарнации на каждый цвет (FIFO).
pub type Reincarnating = HashMap<Color, Vec<ReincarnatingPawn>>;

/// Запас: запасные фигуры каждого цвета.
pub type Reserve = HashMap<Color, Vec<PieceType>>;

/// Добавить срубленную пешку в очередь реинкарнации.
pub fn register_pawn_capture(reincarnating: &mut Reincarnating, color: Color) {
    reincarnating
    .entry(color)
    .or_default()
    .push(ReincarnatingPawn::new());
}

/// Уменьшить счётчики реинкарнации для указанного цвета.
/// Вызывать после хода этого цвета, ДО смены current_player.
///
/// Если в запасе цвета нет пешки — берём первую готовую из очереди.
pub fn tick_reincarnation(
    reincarnating: &mut Reincarnating,
    reserve: &mut Reserve,
    color: Color,
) {
    let queue = reincarnating.entry(color).or_default();
    for entry in queue.iter_mut() {
        if entry.turns_left > 0 {
            entry.turns_left -= 1;
        }
        if entry.turns_left == 0 {
            entry.ready = true;
        }
    }

    // Если в запасе нет пешки — берём первую готовую.
    if !has_pawn_in_reserve(reserve, color) {
        if let Some(i) = queue.iter().position(|e| e.ready) {
            queue.remove(i);
            reserve.entry(color).or_default().push(PieceType::Pawn);
        }
    }
}

/// Если в запасе нет пешки — берём первую готовую из очереди.
/// Вызывать после того, как место в запасе освободилось.
pub fn promote_from_queue(
    reincarnating: &mut Reincarnating,
    reserve: &mut Reserve,
    color: Color,
) {
    if has_pawn_in_reserve(reserve, color) {
        return;
    }
    let queue = reincarnating.entry(color).or_default();
    if let Some(i) = queue.iter().position(|e| e.ready) {
        queue.remove(i);
        reserve.entry(color).or_default().push(PieceType::Pawn);
    }
}

/// Есть ли у цвета пешка в запасе.
pub fn has_pawn_in_reserve(reserve: &Reserve, color: Color) -> bool {
    reserve
    .get(&color)
    .map_or(false, |list| list.contains(&PieceType::Pawn))
}

// ============================================================
// Тесты
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_reincarnating() -> Reincarnating {
        let mut r = HashMap::new();
        r.insert(Color::White, Vec::new());
        r.insert(Color::Black, Vec::new());
        r
    }

    fn empty_reserve() -> Reserve {
        let mut r = HashMap::new();
        r.insert(Color::White, Vec::new());
        r.insert(Color::Black, Vec::new());
        r
    }

    #[test]
    fn register_adds_entry() {
        let mut r = empty_reincarnating();
        register_pawn_capture(&mut r, Color::White);
        assert_eq!(r[&Color::White].len(), 1);
        assert_eq!(r[&Color::White][0].turns_left, REINCARNATION_TURNS);
        assert!(!r[&Color::White][0].ready);
    }

    #[test]
    fn tick_decrements_countdown() {
        let mut r = empty_reincarnating();
        let mut reserve = empty_reserve();
        register_pawn_capture(&mut r, Color::White);

        // 3 тика — счётчик дойдёт до 0.
        tick_reincarnation(&mut r, &mut reserve, Color::White);
        assert_eq!(r[&Color::White][0].turns_left, 2);
        tick_reincarnation(&mut r, &mut reserve, Color::White);
        assert_eq!(r[&Color::White][0].turns_left, 1);
        tick_reincarnation(&mut r, &mut reserve, Color::White);
        // После третьего тика entry готова и её сразу переместили в запас,
        // потому что пешки в запасе не было.
        assert!(r[&Color::White].is_empty());
        assert!(reserve[&Color::White].contains(&PieceType::Pawn));
    }

    #[test]
    fn tick_does_not_move_ready_pawn_if_pawn_in_reserve() {
        let mut r = empty_reincarnating();
        let mut reserve = empty_reserve();
        // Пешка уже в запасе.
        reserve.get_mut(&Color::White).unwrap().push(PieceType::Pawn);
        register_pawn_capture(&mut r, Color::White);

        for _ in 0..3 {
            tick_reincarnation(&mut r, &mut reserve, Color::White);
        }
        // Entry готова, но осталась в очереди — пешка в запасе уже есть.
        assert_eq!(r[&Color::White].len(), 1);
        assert!(r[&Color::White][0].ready);
    }

    #[test]
    fn promote_from_queue_after_slot_freed() {
        let mut r = empty_reincarnating();
        let mut reserve = empty_reserve();
        register_pawn_capture(&mut r, Color::White);

        // Доводим до готовности, но с занятой очередью: сначала займём запас.
        reserve.get_mut(&Color::White).unwrap().push(PieceType::Pawn);
        for _ in 0..3 {
            tick_reincarnation(&mut r, &mut reserve, Color::White);
        }
        assert!(r[&Color::White][0].ready);
        assert!(reserve[&Color::White].contains(&PieceType::Pawn));

        // Освободили запас.
        reserve.get_mut(&Color::White).unwrap().clear();
        promote_from_queue(&mut r, &mut reserve, Color::White);

        assert!(r[&Color::White].is_empty());
        assert!(reserve[&Color::White].contains(&PieceType::Pawn));
    }
}

