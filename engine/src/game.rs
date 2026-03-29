use crate::attack::{self, ClearType};
use crate::board::Board;
use crate::movegen::Placement;
use crate::piece::PieceType;

/// Full game state for the beam search.
#[derive(Clone)]
pub struct GameState {
    pub board: Board,
    pub hold: Option<PieceType>,
    pub hold_used: bool, // whether hold was used this piece (can only hold once per piece)
    pub b2b: bool,
    pub combo: u32,
    pub pieces_placed: u32,
    pub lines_sent: u32,
}

/// Result of placing a piece.
pub struct PlaceResult {
    pub lines_cleared: u32,
    pub clear_type: ClearType,
    pub attack: u32,
    pub is_pc: bool,
}

impl GameState {
    pub fn new() -> Self {
        GameState {
            board: Board::new(),
            hold: None,
            hold_used: false,
            b2b: false,
            combo: 0,
            pieces_placed: 0,
            lines_sent: 0,
        }
    }

    /// Apply a placement to the game state and return the result.
    pub fn apply_placement(&mut self, placement: &Placement) -> PlaceResult {
        // Place the piece
        self.board
            .place_piece(placement.piece, placement.x, placement.y, placement.rot);

        // Clear lines
        let lines_cleared = self.board.clear_lines();

        // Determine clear type
        let clear_type = if lines_cleared > 0 {
            attack::classify_clear(lines_cleared, placement.tspin)
        } else {
            ClearType::None
        };

        // Check perfect clear
        let is_pc = lines_cleared > 0 && self.board.is_empty();
        let final_clear = if is_pc {
            ClearType::PerfectClear
        } else {
            clear_type
        };

        // Calculate attack
        let atk = attack::calculate_attack(final_clear, self.b2b, self.combo, is_pc);

        // Update B2B
        if lines_cleared > 0 {
            if attack::is_b2b_clear(clear_type) {
                self.b2b = true;
            } else if attack::breaks_b2b(clear_type) {
                self.b2b = false;
            }
        }

        // Update combo
        if lines_cleared > 0 {
            self.combo += 1;
        } else {
            self.combo = 0;
        }

        self.pieces_placed += 1;
        self.lines_sent += atk;
        self.hold_used = false;

        PlaceResult {
            lines_cleared,
            clear_type: final_clear,
            attack: atk,
            is_pc,
        }
    }

    /// Swap current piece with hold. Returns the piece to play.
    /// If hold is empty, the held piece becomes current and next from queue is used.
    pub fn do_hold(&mut self, current: PieceType) -> Option<PieceType> {
        if self.hold_used {
            return None;
        }
        self.hold_used = true;
        let prev_hold = self.hold;
        self.hold = Some(current);
        prev_hold
    }

    /// Add garbage lines to the board.
    pub fn add_garbage(&mut self, lines: u32, hole_col: i8) {
        for _ in 0..lines {
            self.board.add_garbage(hole_col);
        }
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}
