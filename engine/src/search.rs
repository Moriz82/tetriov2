use crate::attack;
use crate::eval::{self, Weights};
use crate::game::GameState;
use crate::movegen::{self, Placement};
use crate::piece::PieceType;

#[derive(Clone, Debug)]
pub struct SearchConfig {
    pub beam_width: usize,
    pub weights: Weights,
}

impl Default for SearchConfig {
    fn default() -> Self {
        SearchConfig {
            beam_width: 400,
            weights: Weights::default(),
        }
    }
}

#[derive(Clone)]
struct BeamState {
    board: crate::board::Board,
    hold: Option<PieceType>,
    b2b: bool,
    combo: u32,
    score: f32,
    first_move: Placement,
    first_hold: bool,
    queue_offset: usize, // extra queue pieces consumed (1 if hold-from-empty at root)
}

#[derive(Debug, Clone)]
pub struct BotMove {
    pub placement: Placement,
    pub use_hold: bool,
    pub score: f32,
}

/// Run beam search over the piece queue to find the best first move.
pub fn find_best_move(
    current: PieceType,
    queue: &[PieceType],
    state: &GameState,
    config: &SearchConfig,
) -> Option<BotMove> {
    let mut beam: Vec<BeamState> = Vec::with_capacity(config.beam_width * 50);

    // --- Depth 0: expand root (current piece + hold option) ---
    expand_piece(&state.board, current, state.hold, state.b2b, state.combo, config, &mut beam, false, 0);

    // Hold option: swap current into hold, play the held piece (or queue[0])
    if let Some(held) = state.hold {
        if held != current {
            expand_piece(&state.board, held, Some(current), state.b2b, state.combo, config, &mut beam, true, 0);
        }
    } else if !queue.is_empty() {
        // No hold piece yet: hold current, play queue[0] — consumes one queue slot
        expand_piece(&state.board, queue[0], Some(current), state.b2b, state.combo, config, &mut beam, true, 1);
    }

    if beam.is_empty() {
        return None;
    }

    truncate_beam(&mut beam, config.beam_width);

    // --- Depth 1..N: expand each subsequent queue piece ---
    // max_depth is the queue length, but some states consumed an extra piece (hold-from-empty)
    let max_depth = queue.len();

    for d in 0..max_depth {
        let mut next_beam = Vec::with_capacity(config.beam_width * 50);

        // Use fast movegen for deeper levels (depth > 1), full BFS for depth 0-1
        let use_fast = d >= 2;

        for bs in &beam {
            let qi = d + bs.queue_offset;
            if qi >= queue.len() { continue; } // this state has exhausted the queue
            let piece = queue[qi];

            // Place piece directly
            let placements = if use_fast {
                movegen::generate_placements_fast(&bs.board, piece)
            } else {
                movegen::generate_placements_with_drops(&bs.board, piece)
            };
            for p in &placements {
                if let Some(new_bs) = apply_and_eval(bs, p, bs.hold, config) {
                    next_beam.push(new_bs);
                }
            }

            // Hold swap: hold this piece, play what's in hold
            if let Some(held) = bs.hold {
                if held != piece {
                    let hold_placements = if use_fast {
                        movegen::generate_placements_fast(&bs.board, held)
                    } else {
                        movegen::generate_placements_with_drops(&bs.board, held)
                    };
                    for p in &hold_placements {
                        if let Some(mut new_bs) = apply_and_eval(bs, p, Some(piece), config) {
                            new_bs.hold = Some(piece);
                            next_beam.push(new_bs);
                        }
                    }
                }
            }
        }

        if next_beam.is_empty() {
            break;
        }
        truncate_beam(&mut next_beam, config.beam_width);
        beam = next_beam;
    }

    beam.first().map(|bs| BotMove {
        placement: bs.first_move,
        use_hold: bs.first_hold,
        score: bs.score,
    })
}

/// Expand placements for a piece at a given board state (used for root expansion).
fn expand_piece(
    board: &crate::board::Board,
    piece: PieceType,
    hold_after: Option<PieceType>,
    b2b: bool,
    combo: u32,
    config: &SearchConfig,
    beam: &mut Vec<BeamState>,
    is_hold: bool,
    queue_offset: usize,
) {
    let placements = movegen::generate_placements_with_drops(board, piece);
    for p in &placements {
        let mut new_board = board.clone();
        new_board.place_piece(p.piece, p.x, p.y, p.rot);
        let lines = new_board.clear_lines();

        let clear_type = if lines > 0 {
            attack::classify_clear(lines, p.tspin)
        } else {
            attack::ClearType::None
        };

        let is_pc = lines > 0 && new_board.is_empty();
        let final_clear = if is_pc { attack::ClearType::PerfectClear } else { clear_type };

        let b2b_after = if lines > 0 {
            if attack::is_b2b_clear(clear_type) { true }
            else if attack::breaks_b2b(clear_type) { false }
            else { b2b }
        } else { b2b };

        let combo_after = if lines > 0 { combo + 1 } else { 0 };
        let atk = attack::calculate_attack(final_clear, b2b, combo, is_pc);

        let features = new_board.compute_features();
        let score = eval::evaluate_fast(
            &features, final_clear, p.tspin, lines,
            p.piece == PieceType::T, b2b, b2b_after, combo_after, atk,
            &config.weights,
        );

        beam.push(BeamState {
            board: new_board,
            hold: hold_after,
            b2b: b2b_after,
            combo: combo_after,
            score,
            first_move: *p,
            first_hold: is_hold,
            queue_offset,
        });
    }
}

/// Apply a placement to a beam state and evaluate.
fn apply_and_eval(
    bs: &BeamState,
    placement: &Placement,
    hold_after: Option<PieceType>,
    config: &SearchConfig,
) -> Option<BeamState> {
    let mut board = bs.board.clone();
    board.place_piece(placement.piece, placement.x, placement.y, placement.rot);

    let lines = board.clear_lines();
    let clear_type = if lines > 0 {
        attack::classify_clear(lines, placement.tspin)
    } else {
        attack::ClearType::None
    };

    let is_pc = lines > 0 && board.is_empty();
    let final_clear = if is_pc { attack::ClearType::PerfectClear } else { clear_type };

    let b2b_after = if lines > 0 {
        if attack::is_b2b_clear(clear_type) { true }
        else if attack::breaks_b2b(clear_type) { false }
        else { bs.b2b }
    } else { bs.b2b };

    let combo_after = if lines > 0 { bs.combo + 1 } else { 0 };
    let atk = attack::calculate_attack(final_clear, bs.b2b, bs.combo, is_pc);

    let features = board.compute_features();
    let delta = eval::evaluate_fast(
        &features, final_clear, placement.tspin, lines,
        placement.piece == PieceType::T, bs.b2b, b2b_after, combo_after, atk,
        &config.weights,
    );

    Some(BeamState {
        board,
        hold: hold_after,
        b2b: b2b_after,
        combo: combo_after,
        score: bs.score + delta,
        first_move: bs.first_move,
        first_hold: bs.first_hold,
        queue_offset: bs.queue_offset,
    })
}

fn truncate_beam(beam: &mut Vec<BeamState>, width: usize) {
    let cmp = |a: &BeamState, b: &BeamState| {
        b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
    };
    if beam.len() <= width {
        beam.sort_unstable_by(cmp);
        return;
    }
    beam.select_nth_unstable_by(width - 1, cmp);
    beam.truncate(width);
    beam.sort_unstable_by(cmp);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_move_empty_board() {
        let state = GameState::new();
        let config = SearchConfig {
            beam_width: 100,
            ..Default::default()
        };
        let result = find_best_move(
            PieceType::T,
            &[PieceType::I, PieceType::S, PieceType::Z],
            &state,
            &config,
        );
        assert!(result.is_some());
    }

    #[test]
    fn find_move_prefers_tetris() {
        let mut state = GameState::new();
        for row in 0..4 {
            state.board.rows[row] = 0b0111111111; // cols 0-8 filled, well at 9
        }
        state.board.rebuild_col_heights();

        let config = SearchConfig {
            beam_width: 200,
            ..Default::default()
        };

        let result = find_best_move(PieceType::I, &[PieceType::T], &state, &config);
        assert!(result.is_some());
        let mv = result.unwrap();
        let cells = mv.placement.piece.cells(mv.placement.rot);
        let cols: Vec<i8> = cells.iter().map(|&(dx, _)| mv.placement.x + dx).collect();
        assert!(cols.iter().all(|&c| c == 9), "Expected Tetris at column 9, got cols {:?}", cols);
    }

    #[test]
    fn search_returns_valid_move() {
        let state = GameState::new();
        let config = SearchConfig { beam_width: 50, ..Default::default() };
        let result = find_best_move(PieceType::O, &[PieceType::I], &state, &config);
        assert!(result.is_some());
        let mv = result.unwrap();
        // Placement should be valid (on the board, not colliding)
        assert!(!state.board.collides(mv.placement.piece, mv.placement.x, mv.placement.y, mv.placement.rot));
    }
}
