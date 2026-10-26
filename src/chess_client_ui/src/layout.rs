//! Раскладка окна: границы панелей и размер клетки.

use chess_logic::GameMode;

pub const MARGIN: u32 = 20;
pub const LEFT_PANEL: u32 = 220;
pub const RIGHT_PANEL: u32 = 120;
pub const TOP_MARGIN: u32 = 20;
pub const BOTTOM_MARGIN: u32 = 30;

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub cell: u32,
    pub board_x: u32,
    pub board_y: u32,
    pub board_w: u32,
    pub board_h: u32,
    pub mode: GameMode,
}

impl Layout {
    pub fn from_window(width: u32, height: u32, mode: GameMode) -> Self {
        let available_w = width.saturating_sub(LEFT_PANEL + RIGHT_PANEL + 2 * MARGIN);
        let available_h = height.saturating_sub(TOP_MARGIN + BOTTOM_MARGIN + 2 * MARGIN);

        let (cols, rows) = match mode {
            GameMode::Classic8x8 => (8, 8),
            GameMode::Duel => (10, 10),    // <-- 10
            GameMode::Team2v2 => (16, 10),
        };
        // Защита: минимум 1 пиксель на клетку.
        let cell_w = (available_w / cols).max(1);
        let cell_h = (available_h / rows).max(1);
        let cell = cell_w.min(cell_h).max(1);

        let board_w = cell * cols;
        let board_h = cell * rows;

        let board_x = LEFT_PANEL + MARGIN;
        let board_y = TOP_MARGIN;

        Self { cell, board_x, board_y, board_w, board_h, mode }
    }

    pub fn cols(&self) -> u32 {
        match self.mode {
            GameMode::Classic8x8 => 8,
            GameMode::Duel => 10,       // <-- 10
            GameMode::Team2v2 => 16,
        }
    }

    pub fn rows(&self) -> u32 {
        match self.mode {
            GameMode::Classic8x8 => 8,
            GameMode::Duel => 10,
            GameMode::Team2v2 => 10,
        }
    }

    pub fn left(&self) -> f32 { self.board_x as f32 }
    pub fn top(&self) -> f32 { self.board_y as f32 }
    pub fn right(&self) -> f32 { (self.board_x + self.board_w) as f32 }
    pub fn bottom(&self) -> f32 { (self.board_y + self.board_h) as f32 }

    pub fn cell_origin(&self, col: u32, row: u32) -> (f32, f32) {
        let x = self.left() + col as f32 * self.cell as f32;
        let y = self.top() + (self.rows() - 1 - row) as f32 * self.cell as f32;
        (x, y)
    }

    /// По координатам мыши вернуть клетку (col, row).
    pub fn pixel_to_cell(&self, x: f64, y: f64) -> Option<(u32, u32)> {
        let lx = self.left() as f64;
        let ly = self.top() as f64;
        let size_w = self.board_w as f64;
        let size_h = self.board_h as f64;

        if x < lx || y < ly || x >= lx + size_w || y >= ly + size_h {
            return None;
        }

        let cell = self.cell as f64;
        let col = ((x - lx) / cell).floor() as i64;
        let row_from_top = ((y - ly) / cell).floor() as i64;
        let row = (self.rows() as i64 - 1) - row_from_top;

        if col < 0 || col >= self.cols() as i64 || row < 0 || row >= self.rows() as i64 {
            return None;
        }
        Some((col as u32, row as u32))
    }
}
