//! Шахматные часы.

use crate::piece::Color;
use std::time::{Duration, Instant};

/// Часы: остаток у белых/чёрных, инкремент за ход, момент последнего тика.
///
/// `None` в GameState — партия без таймера.
#[derive(Clone, Debug, PartialEq)]
pub struct Clock {
    pub white: Duration,
    pub black: Duration,
    pub increment: Duration,
    pub last_move: Instant,
}

impl Clock {
    pub fn new(initial: Duration, increment: Duration) -> Self {
        Self {
            white: initial,
            black: initial,
            increment,
            last_move: Instant::now(),
        }
    }

    /// Остаток для цвета.
    pub fn remaining(&self, color: Color) -> Duration {
        match color {
            Color::White => self.white,
            Color::Black => self.black,
        }
    }

    /// Уменьшить время текущего игрока на `elapsed`, обновить `last_move`.
    /// Возвращает true, если время истекло у текущего игрока.
    ///
    /// Не устанавливает Winner — это делает GameState.
    pub fn decrement(&mut self, color: Color, elapsed: Duration) -> bool {
        let elapsed = if elapsed < Duration::ZERO {
            Duration::ZERO
        } else {
            elapsed
        };

        match color {
            Color::White => {
                if self.white <= elapsed {
                    self.white = Duration::ZERO;
                    true
                } else {
                    self.white -= elapsed;
                    false
                }
            }
            Color::Black => {
                if self.black <= elapsed {
                    self.black = Duration::ZERO;
                    true
                } else {
                    self.black -= elapsed;
                    false
                }
            }
        }
    }

    /// Добавить инкремент текущему игроку. Вызывать после успешного хода,
    /// до смены current_player.
    pub fn apply_increment(&mut self, color: Color) {
        match color {
            Color::White => self.white += self.increment,
            Color::Black => self.black += self.increment,
        }
        self.last_move = Instant::now();
    }

    /// Сбросить `last_move` в текущий момент (без изменения остатка).
    pub fn reset_last_move(&mut self) {
        self.last_move = Instant::now();
    }
}

// ============================================================
// Тесты
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_clock_sets_both_colors() {
        let c = Clock::new(Duration::from_secs(180), Duration::from_secs(2));
        assert_eq!(c.white, Duration::from_secs(180));
        assert_eq!(c.black, Duration::from_secs(180));
        assert_eq!(c.increment, Duration::from_secs(2));
    }

    #[test]
    fn remaining_returns_correct_color() {
        let mut c = Clock::new(Duration::from_secs(100), Duration::ZERO);
        c.white = Duration::from_secs(80);
        c.black = Duration::from_secs(90);
        assert_eq!(c.remaining(Color::White), Duration::from_secs(80));
        assert_eq!(c.remaining(Color::Black), Duration::from_secs(90));
    }

    #[test]
    fn decrement_reduces_current_player_only() {
        let mut c = Clock::new(Duration::from_secs(10), Duration::ZERO);
        c.decrement(Color::White, Duration::from_secs(1));
        assert_eq!(c.white, Duration::from_secs(9));
        assert_eq!(c.black, Duration::from_secs(10));
    }

    #[test]
    fn decrement_flags_timeout() {
        let mut c = Clock::new(Duration::from_millis(100), Duration::ZERO);
        let expired = c.decrement(Color::White, Duration::from_secs(1));
        assert!(expired);
        assert_eq!(c.white, Duration::ZERO);
    }

    #[test]
    fn decrement_does_not_go_below_zero() {
        let mut c = Clock::new(Duration::from_millis(100), Duration::ZERO);
        c.decrement(Color::White, Duration::from_secs(1));
        assert_eq!(c.white, Duration::ZERO);
    }

    #[test]
    fn apply_increment_adds_time() {
        let mut c = Clock::new(Duration::from_secs(10), Duration::from_secs(2));
        c.apply_increment(Color::White);
        assert_eq!(c.white, Duration::from_secs(12));
        assert_eq!(c.black, Duration::from_secs(10));
    }
}
