//! Логика игры 10×10 (duel) и 16×10 (team_2v2).

pub mod attack;
pub mod board;
pub mod castle;
pub mod clock;
pub mod freestyle;
pub mod json;
pub mod moves;
pub mod piece;
pub mod reincarnation;
pub mod state;
pub mod team;

pub use attack::{
    filter_legal_moves, has_any_king_attacked, has_any_legal_move, has_in_reserve,
    has_any_move_to_save_royal, in_check, is_checkmate, is_player_royal_attacked,
    is_square_attacked, is_square_attacked_by_piece, legal_drops,
    legal_royal_return_squares, remove_from_reserve,
};
pub use board::Board;
pub use castle::{apply_castle, can_castle, compute_castle_moves, is_king_castle_safe};
pub use clock::Clock;
pub use freestyle::{
    apply_random_position, apply_random_position_8x8, random_back_rank, random_back_rank_8x8,
};
pub use moves::{get_moves, EnPassantMove};
pub use piece::{
    in_bounds, Color, Piece, PieceType, PlayerId, Seat, Square,
    BOARD_COLS, BOARD_ROWS, BOARD_SIZE,
};
pub use reincarnation::{
    has_pawn_in_reserve, promote_from_queue, register_pawn_capture, tick_reincarnation,
    Reincarnating, ReincarnatingPawn, Reserve, REINCARNATION_TURNS,
};
pub use state::{EndReason, GameAction, GameMode, GameState, MoveError, MoveResult};
pub use team::{PlayerState, TeamState};
