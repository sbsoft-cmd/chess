use crate::attack::{
    filter_legal_moves, filter_legal_moves_2v2, has_any_king_attacked, has_any_legal_move,
    has_any_move_to_save_royal, in_check, is_checkmate, is_player_royal_attacked,
    is_square_attacked, legal_drops as attack_legal_drops,
};
use crate::moves::get_moves;

use crate::board::Board;

use crate::castle::{
    apply_castle as apply_castle_swap,
    apply_castle_classic,
    compute_castle_moves,
};

use crate::clock::Clock;
use crate::moves::EnPassantMove;
use crate::piece::{Color, Piece, PieceType, Seat, Square, BOARD_COLS, BOARD_ROWS};
use crate::reincarnation::{
    promote_from_queue, register_pawn_capture, tick_reincarnation, Reincarnating, Reserve,
};
use crate::team::TeamState;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

// ============================================================
// HistorySnapshot
// ============================================================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistorySnapshot {
    pub board: Board,
    pub current_player: Color,
    pub white_team: TeamState,
    pub black_team: TeamState,
    #[serde(default)]
    pub reserve: Reserve,
    #[serde(default)]
    pub reincarnating: Reincarnating,
}

// ============================================================
// GameMode
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameMode {
    Duel,
    Team2v2,
    #[serde(rename = "classic_8x8")]
    Classic8x8,
}

impl GameMode {
    pub fn is_team(self) -> bool { matches!(self, GameMode::Team2v2) }
}

// ============================================================
// EndReason
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    Checkmate,
    Stalemate,
    Resign,
    Draw,
    Timeout,
    Coronation,
}

// ============================================================
// GameAction
// ============================================================

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameAction {
    Move { from: Square, to: Square, promotion: Option<PieceType> },
    Castle { a: Square, c: Square },
    Drop { piece: PieceType, to: Square },
    ReturnRoyal { royal_pt: PieceType, to: Square },
    Resign,
}

// ============================================================
// MoveError / MoveResult
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveError {
    GameOver,
    NotYourColor,
    NotYourSeat,
    IllegalMove,
    NotYourPiece,
    ResignNotAllowed,
    RoyalInHolding(PieceType),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MoveResult {
    pub ok: bool,
    pub captured: Option<Piece>,
    pub promotion: bool,
    pub checkmate: bool,
    pub stalemate: bool,
    pub check: bool,
    pub drop: bool,
    pub coronation: bool,
    pub royal_to_holding: Option<(Seat, PieceType)>,
    pub error: Option<MoveError>,
}

impl MoveResult {
    pub fn err(e: MoveError) -> Self { Self { error: Some(e), ..Default::default() } }
}

// ============================================================
// GameState
// ============================================================

#[derive(Clone, Debug)]
pub struct GameState {
    pub mode: GameMode,
    pub board: Board,
    pub current_player: Color,
    pub moved_pawns: HashSet<Square>,
    pub en_passant: Option<EnPassantMove>,
    pub winner: Option<Color>,
    pub is_draw: bool,
    pub resigned_by: Option<Color>,
    pub end_reason: Option<EndReason>,
    pub reserve: Reserve,
    pub clock: Option<Clock>,
    pub first_move_made: bool,
    pub reincarnating: Reincarnating,
    pub white_team: TeamState,
    pub black_team: TeamState,
    pub history: Vec<HistorySnapshot>,
}

impl GameState {
    pub fn new() -> Self { Self::new_with_clock(0, 0) }

    pub fn new_with_clock(initial_secs: u64, increment_secs: u64) -> Self {
        let mut reserve: Reserve = HashMap::new();
        reserve.insert(Color::White, vec![PieceType::Cheetah, PieceType::Genie]);
        reserve.insert(Color::Black, vec![PieceType::Cheetah, PieceType::Genie]);

        let mut reincarnating: Reincarnating = HashMap::new();
        reincarnating.insert(Color::White, Vec::new());
        reincarnating.insert(Color::Black, Vec::new());

        let clock = if initial_secs > 0 {
            Some(Clock::new(Duration::from_secs(initial_secs), Duration::from_secs(increment_secs)))
        } else { None };

        Self {
            mode: GameMode::Duel,
            board: Board::initial(),
            current_player: Color::White,
            moved_pawns: HashSet::new(),
            en_passant: None,
            winner: None,
            is_draw: false,
            resigned_by: None,
            end_reason: None,
            reserve,
            clock,
            first_move_made: false,
            reincarnating,
            white_team: TeamState::new(),
            black_team: TeamState::new(),
            history: Vec::new(),
        }
    }

    pub fn new_team_2v2_with_clock(initial_secs: u64, increment_secs: u64) -> Self {
        let mut s = Self::new_with_clock(initial_secs, increment_secs);
        s.mode = GameMode::Team2v2;
        s.board = Board::initial_team_2v2();
        s.reserve.clear();
        s.reincarnating.clear();
        s.history.clear();
        s
    }

    pub fn new_classic_8x8_with_clock(initial_secs: u64, increment_secs: u64) -> Self {
        let mut s = Self::new_with_clock(initial_secs, increment_secs);
        s.mode = GameMode::Classic8x8;
        s.board = Board::initial_classic_8x8();
        s
    }

    pub fn new_classic_8x8_freestyle_with_clock(initial_secs: u64, increment_secs: u64) -> Self {
        let mut s = Self::new_classic_8x8_with_clock(initial_secs, increment_secs);
        crate::freestyle::apply_random_position_8x8(&mut s.board);
        s
    }

    fn post_move_checks(&mut self) {
        if self.is_over() { return; }
        if self.mode == GameMode::Team2v2 {
            self.check_team2v2_end();
        } else {
            let color = self.current_player;
            if in_check(&self.board, color) {
                if is_checkmate(&self.board, color, &self.moved_pawns, self.en_passant.as_ref(), &self.reserve) {
                    self.winner = Some(color.opposite());
                    self.end_reason = Some(EndReason::Checkmate);
                } else if !has_any_legal_move(&self.board, color, &self.moved_pawns, self.en_passant.as_ref(), &self.reserve) {
                    self.is_draw = true;
                    self.end_reason = Some(EndReason::Stalemate);
                }
            } else if !has_any_legal_move(&self.board, color, &self.moved_pawns, self.en_passant.as_ref(), &self.reserve) {
                self.is_draw = true;
                self.end_reason = Some(EndReason::Stalemate);
            }
        }
    }

    // ============================================================
    // Служебное
    // ============================================================

    pub fn is_over(&self) -> bool {
        self.winner.is_some() || self.is_draw || self.resigned_by.is_some()
    }

    pub fn team(&self, color: Color) -> &TeamState {
        match color {
            Color::White => &self.white_team,
            Color::Black => &self.black_team,
        }
    }

    pub fn team_mut(&mut self, color: Color) -> &mut TeamState {
        match color {
            Color::White => &mut self.white_team,
            Color::Black => &mut self.black_team,
        }
    }

    fn save_history(&mut self) {
        if self.history.len() < 1000 {
            self.history.push(HistorySnapshot {
                board: self.board.clone(),
                              current_player: self.current_player,
                              white_team: self.white_team.clone(),
                              black_team: self.black_team.clone(),
                              reserve: self.reserve.clone(),
                              reincarnating: self.reincarnating.clone(),
            });
        }
    }

    // ============================================================
    // Легальные ходы
    // ============================================================

    pub fn legal_moves(&self, from: Square) -> Vec<Square> {
        if self.is_over() { return Vec::new(); }
        let p = match self.board.get(from) { Some(p) => p, None => return Vec::new() };
        if p.color != self.current_player { return Vec::new(); }

        if self.mode == GameMode::Team2v2 {
            get_moves(&self.board, from, &self.moved_pawns, self.en_passant.as_ref())
        } else {
            filter_legal_moves(&self.board, from, &self.moved_pawns, self.en_passant.as_ref())
        }
    }

    pub fn legal_castles(&self, from: Square) -> Vec<Square> {
        if self.is_over() { return Vec::new(); }
        let p = match self.board.get(from) { Some(p) => p, None => return Vec::new() };
        if p.color != self.current_player { return Vec::new(); }
        compute_castle_moves(&self.board, from, p.color)
    }

    pub fn legal_drops(&self, piece_type: PieceType) -> Vec<Square> {
        if self.mode == GameMode::Classic8x8 { return Vec::new(); }
        if self.is_over() { return Vec::new(); }
        attack_legal_drops(&self.board, &self.reserve, self.current_player, piece_type)
    }

    pub fn legal_royal_return_squares(&self, color: Color) -> Vec<Square> {
        let step: i8 = if color == Color::White { 1 } else { -1 };
        let mut row = color.back_row(self.board.rows());

        while (0..BOARD_ROWS).contains(&row) {
            let mut row_squares = Vec::new();
            for col in 0..BOARD_COLS {
                let sq = Square::new(col, row);
                if self.board.get(sq).is_none() {
                    row_squares.push(sq);
                }
            }
            if !row_squares.is_empty() {
                return row_squares;
            }
            row += step;
        }
        Vec::new()
    }

    // ============================================================
    // apply_action
    // ============================================================

    pub fn apply_action(&mut self, color: Color, seat: Seat, action: GameAction) -> MoveResult {
        //println!(
        //    "[apply_action] Вход: color={:?}, seat={:?}, action={:?}, current_player={:?}, moves_made={}",
        //    color, seat, action, self.current_player,
        //    if self.mode.is_team() { self.team(color).moves_made_this_turn } else { 0 }
        //);

        if self.is_over() {
            eprintln!("[apply_action] Отказ: игра окончена");
            return MoveResult::err(MoveError::GameOver);
        }
        if self.current_player != color {
            eprintln!("[apply_action] Отказ: сейчас не ваш цвет");
            return MoveResult::err(MoveError::NotYourColor);
        }

        if self.mode.is_team() && !self.team(color).player(seat).alive {
            eprintln!("[apply_action] Отказ: этот Seat выбыл");
            return MoveResult::err(MoveError::NotYourSeat);
        }

        if self.mode.is_team() {
            let team = self.team(color);
            if team.last_seat_moved == Some(seat) {
                eprintln!("[apply_action] Отказ: этот игрок уже ходил в этом окне");
                return MoveResult::err(MoveError::IllegalMove);
            }
        }

        if self.mode.is_team() {
            let p = self.team(color).player(seat);
            if let Some(pt) = p.must_return_royal() {
                if !matches!(action, GameAction::ReturnRoyal { .. }) {
                    return MoveResult::err(MoveError::IllegalMove);
                }
            } else if p.must_skip_turn() && !matches!(action, GameAction::ReturnRoyal { .. }) {
                let pt = if p.prince_in_holding { PieceType::Prince } else { PieceType::Princess };
                return MoveResult::err(MoveError::RoyalInHolding(pt));
            }
        }

        self.save_history();

        if matches!(action, GameAction::Resign) {
            if self.mode == GameMode::Team2v2 {
                return MoveResult::err(MoveError::ResignNotAllowed);
            }
            return self.resign(color);
        }

        if let GameAction::ReturnRoyal { royal_pt, to } = action {
            let res = self.apply_return_royal(color, seat, royal_pt, to);
            if res.ok {
                self.finish_turn(color, seat);
            }
            return res;
        }

        let res = match action {
            GameAction::Move { from, to, promotion } => {
                if self.mode.is_team() && self.board.get_owner(from) != Some(seat) {
                    // Фикс: если owner потерян — восстанавливаем из seat.
                    eprintln!("[apply_action] ВНИМАНИЕ: owner({:?})={:?} != seat {:?}, восстанавливаем", from, self.board.get_owner(from), seat);
                    self.board.set_owner(from, Some(seat));
                }
                self.apply_move_internal(color, seat, from, to, promotion)
            }
            GameAction::Castle { a, c } => self.apply_castle_internal(color, a, c),
            GameAction::Drop { piece, to } => self.apply_drop_internal(color, piece, to),
            GameAction::ReturnRoyal { .. } => unreachable!(),
            GameAction::Resign => unreachable!(),
        };

        if res.ok {
            self.finish_turn(color, seat);
        } else {
            eprintln!("[apply_action] Ход отклонен: {:?}", res.error);
        }

        res
    }

    fn skip_turns_if_needed(&mut self) {
        if !self.mode.is_team() { return; }

        let mut safety = 0;
        loop {
            safety += 1;
            if safety > 4 { break; }

            let current = self.current_player;
            let team = self.team(current);

            if team.active_players_count() > 0 { break; }

            let team = self.team_mut(current);
            for p in team.players.iter_mut() {
                if p.prince_in_holding && p.prince_cd > 0 { p.prince_cd -= 1; }
                if p.princess_in_holding && p.princess_cd > 0 { p.princess_cd -= 1; }
            }
            team.reset_turn_counters();

            self.current_player = current.opposite();
        }
    }

    // ============================================================
    // Внутренние применения ходов
    // ============================================================

    fn apply_move_internal(
        &mut self,
        color: Color,
        seat: Seat,
        from: Square,
        to: Square,
        promotion: Option<PieceType>,
    ) -> MoveResult {
        let mut res = MoveResult::default();
        let piece = match self.board.get(from) { Some(p) => p, None => return res };
        if piece.color != color { return res; }

        let legal = if self.mode == GameMode::Team2v2 {
            get_moves(&self.board, from, &self.moved_pawns, self.en_passant.as_ref())
        } else {
            filter_legal_moves(&self.board, from, &self.moved_pawns, self.en_passant.as_ref())
        };

        if !legal.contains(&to) { return res; }

        if let Some(target) = self.board.get(to) {
            if target.piece_type == PieceType::King && target.color != color && self.mode == GameMode::Duel {
                return res;
            }
        }

        // Фикс: если from_owner потерян, восстанавливаем из seat (в 2×2).
        let from_owner = self.board.get_owner(from).or_else(|| {
            if self.mode.is_team() { Some(seat) } else { None }
        });

        if let Some(ep) = self.en_passant.clone() {
            if piece.piece_type == PieceType::Pawn && ep.pawn_color != color {
                for &crossed in &ep.crossed {
                    if to == crossed {
                        let cap_sq = Square::new(ep.to_col, ep.to_row);
                        if let Some(cap) = self.board.remove(cap_sq) {
                            res.captured = Some(cap);
                            if self.mode == GameMode::Duel && cap.piece_type == PieceType::Pawn {
                                register_pawn_capture(&mut self.reincarnating, cap.color);
                            }
                        }
                        break;
                    }
                }
            }
        }

        if let Some(target) = self.board.get(to) {
            let owner = self.board.get_owner(to);
            if let Some(cap) = self.board.remove(to) {
                res.captured = Some(cap);

                if self.mode == GameMode::Duel && cap.piece_type == PieceType::Pawn {
                    register_pawn_capture(&mut self.reincarnating, cap.color);
                }

                if self.mode == GameMode::Team2v2 && cap.piece_type.is_royal_2v2() {
                    if let Some(o) = owner {
                        let p = self.team_mut(cap.color).player_mut(o);
                        if cap.piece_type == PieceType::Prince {
                            p.prince_hits += 1;
                            p.prince_in_holding = true;
                            p.prince_cd = p.prince_hits;
                        } else {
                            p.princess_hits += 1;
                            p.princess_in_holding = true;
                            p.princess_cd = p.princess_hits;
                        }
                        res.royal_to_holding = Some((o, cap.piece_type));
                    }
                }
            }
        }

        self.board.remove(from);
        self.board.set_with_owner(to, Some(piece), from_owner);

        let mut new_ep: Option<EnPassantMove> = None;
        if piece.piece_type == PieceType::Pawn {
            let dr = to.row - from.row;
            let is_first_move = !self.moved_pawns.contains(&from);
            if is_first_move && dr.abs() >= 2 {
                let step = if to.row < from.row { -1 } else { 1 };
                let mut crossed = Vec::new();
                let mut r = from.row + step;
                while r != to.row {
                    crossed.push(Square::new(from.col, r));
                    r += step;
                }
                new_ep = Some(EnPassantMove {
                    from_col: from.col,
                    from_row: from.row,
                    to_col: to.col,
                    to_row: to.row,
                    pawn_color: piece.color,
                    crossed,
                });
            }
            self.moved_pawns.insert(to);
        }

        if piece.piece_type == PieceType::Pawn && to.row == color.pawn_promotion_row(self.board.rows()) {
            let promo = promotion.unwrap_or(PieceType::Queen);
            // Фикс: сохраняем owner при превращении.
            self.board.set_with_owner(to, Some(Piece::new(color, promo)), from_owner);
            res.promotion = true;
        }

        self.en_passant = new_ep;
        self.first_move_made = true;

        if self.mode.is_team() && piece.piece_type.is_royal_2v2() && to.row == color.pawn_promotion_row(self.board.rows()) {
            self.winner = Some(color);
            self.end_reason = Some(EndReason::Coronation);
            res.coronation = true;
            res.ok = true;
            return res;
        }

        res.ok = true;
        res
    }

    fn apply_drop_internal(&mut self, color: Color, piece_type: PieceType, to: Square) -> MoveResult {
        let mut res = MoveResult { drop: true, ..Default::default() };

        if !self.reserve.get(&color).map_or(false, |v| v.contains(&piece_type)) { return res; }
        if !to.in_bounds() || self.board.get(to).is_some() { return res; }
        if !self.board.in_bounds(to) { return res; }

        if piece_type == PieceType::Pawn {
            if to.row != color.pawn_start_row(self.board.rows()) { return res; }
            for r in 0..self.board.rows() {
                if let Some(p) = self.board.get(Square::new(to.col, r)) {
                    if p.color == color && p.piece_type == PieceType::Pawn { return res; }
                }
            }
        } else if !color.is_on_own_half(to.row, self.board.rows()) { return res; }

        let mut test = self.board.clone();
        test.set(to, Some(Piece::new(color, piece_type)));
        if has_any_king_attacked(&test, color) { return res; }

        if let Some(list) = self.reserve.get_mut(&color) {
            if let Some(i) = list.iter().position(|&x| x == piece_type) { list.remove(i); }
        }
        // Drop — только 1×1, owner не важен.
        self.board.set(to, Some(Piece::new(color, piece_type)));
        self.en_passant = None;
        self.first_move_made = true;

        if piece_type == PieceType::Pawn {
            promote_from_queue(&mut self.reincarnating, &mut self.reserve, color);
        }
        res.ok = true;
        res
    }

    fn apply_castle_internal(&mut self, color: Color, a: Square, c: Square) -> MoveResult {
        let mut res = MoveResult::default();
        let mut a2 = a;
        let mut c2 = c;

        let possible = compute_castle_moves(&self.board, a2, color);
        if !possible.contains(&c2) {
            let possible2 = compute_castle_moves(&self.board, c2, color);
            if possible2.contains(&a2) {
                std::mem::swap(&mut a2, &mut c2);
            } else {
                return res;
            }
        }

        let oa = self.board.get_owner(a2);
        let oc = self.board.get_owner(c2);

        if self.mode == GameMode::Team2v2 || self.board.cols() >= 10 {
            // 10×10 и 2×2: обмен местами.
            apply_castle_swap(&mut self.board, a2, c2);
            self.board.set_owner(a2, oc);
            self.board.set_owner(c2, oa);
        } else {
            // 8×8: Король + Ладья (Фишер).
            let a_piece = self.board.get(a2);
            let c_piece = self.board.get(c2);
            let a_is_king = a_piece.map_or(false, |p| p.piece_type == PieceType::King);
            let (king_sq, rook_sq) = if a_is_king { (a2, c2) } else { (c2, a2) };
            apply_castle_classic(&mut self.board, king_sq, rook_sq);
            // Восстановим owner'ы.
            if let (Some(k), Some(r)) = (oa.or(oc), oc.or(oa)) {
                let back = color.back_row(self.board.rows());
                let kingside = self.board.get(king_sq).map_or(true, |p| p.piece_type == PieceType::King);
                let _ = (k, r, back, kingside);
            }
            // Упрощённо: owner'ы не критичны в 8×8 (1×1 без Seat).
        }

        self.en_passant = None;
        self.first_move_made = true;
        res.ok = true;
        res
    }

    fn apply_return_royal(&mut self, color: Color, seat: Seat, royal_pt: PieceType, to: Square) -> MoveResult {
        let mut res = MoveResult::default();
        let p = self.team(color).player(seat);

        if !p.is_in_holding(royal_pt) || p.get_cd(royal_pt) > 0 {
            return MoveResult::err(MoveError::IllegalMove);
        }

        let valid_squares = self.legal_royal_return_squares(color);
        if !valid_squares.contains(&to) {
            return MoveResult::err(MoveError::IllegalMove);
        }

        // Возвращаем Royal на доску с owner = Some(seat).
        self.board.set_with_owner(to, Some(Piece::new(color, royal_pt)), Some(seat));

        let p = self.team_mut(color).player_mut(seat);
        if royal_pt == PieceType::Prince {
            p.prince_in_holding = false;
        } else {
            p.princess_in_holding = false;
        }

        res.ok = true;
        res
    }

    fn finish_turn(&mut self, color: Color, seat: Seat) {
        if self.mode.is_team() {
            let team = self.team_mut(color);
            for p in team.players.iter_mut() {
                if p.prince_in_holding && p.prince_cd > 0 { p.prince_cd -= 1; }
                if p.princess_in_holding && p.princess_cd > 0 { p.princess_cd -= 1; }
            }

            team.moves_made_this_turn += 1;
            team.last_seat_moved = Some(seat);

            let active_count = team.active_players_count();
            let max_moves = if active_count >= 2 { 2 } else { 1 };

            if team.moves_made_this_turn >= max_moves {
                team.reset_turn_counters();
                if let Some(clock) = self.clock.as_mut() {
                    clock.apply_increment(color);
                }
                self.current_player = color.opposite();
            }
        } else {
            if let Some(clock) = self.clock.as_mut() {
                clock.apply_increment(color);
            }
            self.current_player = color.opposite();
        }

        self.skip_turns_if_needed();

        if self.mode == GameMode::Duel {
            tick_reincarnation(&mut self.reincarnating, &mut self.reserve, color);
        }

        self.post_move_checks();
    }

    fn check_team2v2_end(&mut self) {
        for color in [Color::White, Color::Black] {
            let promo_row = color.pawn_promotion_row(self.board.rows());
            for (sq, p) in self.board.iter_pieces() {
                if p.color == color && p.piece_type.is_royal_2v2() && sq.row == promo_row {
                    self.winner = Some(color);
                    self.end_reason = Some(EndReason::Coronation);
                    return;
                }
            }
        }
    }

    // ============================================================
    // Сдача / Ничья / Часы
    // ============================================================

    pub fn resign(&mut self, color: Color) -> MoveResult {
        if self.is_over() { return MoveResult::err(MoveError::GameOver); }
        if self.mode == GameMode::Team2v2 {
            self.winner = Some(color.opposite());
            self.end_reason = Some(EndReason::Resign);
            let mut res = MoveResult::default();
            res.ok = true;
            return res;
        }
        self.resigned_by = Some(color);
        self.winner = Some(color.opposite());
        self.end_reason = Some(EndReason::Resign);
        let mut res = MoveResult::default();
        res.ok = true;
        res
    }

    pub fn agree_draw(&mut self) {
        if self.is_over() { return; }
        self.is_draw = true;
        self.end_reason = Some(EndReason::Draw);
    }

    pub fn tick(&mut self) -> bool {
        if self.clock.is_none() { return false; }
        if self.is_over() {
            if let Some(c) = self.clock.as_mut() { c.reset_last_move(); }
            return false;
        }
        let color = self.current_player;
        let expired = {
            let clock = self.clock.as_mut().unwrap();
            let elapsed = clock.last_move.elapsed();
            let exp = clock.decrement(color, elapsed);
            clock.reset_last_move();
            exp
        };
        if expired {
            self.winner = Some(color.opposite());
            self.end_reason = Some(EndReason::Timeout);
        }
        expired
    }

    pub fn apply_move(&mut self, from: Square, to: Square, promotion: Option<PieceType>) -> MoveResult {
        if self.is_over() { return MoveResult::default(); }
        let color = self.current_player;
        if self.board.get(from).map(|p| p.color) != Some(color) { return MoveResult::default(); }
        self.save_history();
        let res = self.apply_move_internal(color, Seat::First, from, to, promotion);
        if res.ok { self.finish_turn(color, Seat::First); }
        res
    }

    pub fn apply_castle(&mut self, a: Square, c: Square) -> MoveResult {
        if self.is_over() { return MoveResult::default(); }
        let color = self.current_player;
        self.save_history();
        let res = self.apply_castle_internal(color, a, c);
        if res.ok { self.finish_turn(color, Seat::First); }
        res
    }

    pub fn apply_drop(&mut self, color: Color, piece_type: PieceType, to: Square) -> MoveResult {
        if self.is_over() || color != self.current_player { return MoveResult::default(); }
        self.save_history();
        let res = self.apply_drop_internal(color, piece_type, to);
        if res.ok { self.finish_turn(color, Seat::First); }
        res
    }
}
