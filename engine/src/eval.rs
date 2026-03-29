use crate::attack::ClearType;
use crate::board::{Board, BoardFeatures};
use crate::movegen::TSpinType;

/// Tunable evaluation weights. Higher = better.
#[derive(Clone, Debug)]
pub struct Weights {
    // Board quality penalties (negative)
    pub max_height: f32,
    pub max_height_sq: f32,  // quadratic penalty for extreme height
    pub sum_height: f32,
    pub holes: f32,
    pub covered_cells: f32,
    pub bumpiness: f32,
    pub bumpiness_sq: f32,
    pub col_transitions: f32,
    pub row_transitions: f32,

    // Well management
    pub well_depth: f32,
    pub excess_wells: f32,

    // Line clear rewards
    pub single: f32,
    pub double: f32,
    pub triple: f32,
    pub tetris: f32,
    pub tspin_mini_single: f32,
    pub tspin_single: f32,
    pub tspin_double: f32,
    pub tspin_triple: f32,
    pub perfect_clear: f32,

    // Strategic
    pub b2b_maintain: f32,
    pub b2b_break: f32,
    pub combo: f32,
    pub wasted_t: f32,
    pub edge_well_bonus: f32,   // bonus when well is on column 0 or 9

    // Danger thresholds
    pub danger_threshold: f32,
    pub danger_height_mult: f32,
    pub danger_holes_mult: f32,
}

impl Weights {
    /// Tuned for aggressive S+ play: stack for Tetrises and T-spins.
    pub fn default_aggressive() -> Self {
        Weights {
            // Allow moderate height - we NEED height to do Tetrises
            max_height: -0.15,
            max_height_sq: -0.008,
            sum_height: -0.02,
            // Holes are devastating
            holes: -6.0,
            covered_cells: -1.0,
            // Keep surface flat (except the well)
            bumpiness: -0.15,
            bumpiness_sq: -0.02,
            col_transitions: -0.15,
            row_transitions: -0.15,

            // Well management: STRONGLY reward keeping one deep well
            well_depth: 1.5,
            excess_wells: -3.0,

            // Line clears: heavily penalize wasteful clears, reward efficient ones
            single: -3.0,         // singles are terrible (waste stacking, break B2B)
            double: -1.0,         // doubles are still wasteful
            triple: 0.5,
            tetris: 15.0,         // Tetris is king
            tspin_mini_single: 2.0,
            tspin_single: 7.0,
            tspin_double: 16.0,
            tspin_triple: 20.0,
            perfect_clear: 40.0,

            b2b_maintain: 5.0,
            b2b_break: -5.0,
            combo: 1.2,
            wasted_t: -4.0,
            edge_well_bonus: 2.0,  // strongly prefer well on edge

            danger_threshold: 15.0,
            danger_height_mult: 5.0,
            danger_holes_mult: 4.0,
        }
    }

    pub fn default_survival() -> Self {
        Weights {
            max_height: -1.2,
            max_height_sq: -0.05,
            sum_height: -0.2,
            holes: -8.0,
            covered_cells: -2.0,
            bumpiness: -0.5,
            bumpiness_sq: -0.08,
            col_transitions: -0.6,
            row_transitions: -0.5,

            well_depth: 0.1,
            excess_wells: -2.0,

            single: 0.5,
            double: 1.5,
            triple: 3.0,
            tetris: 5.0,
            tspin_mini_single: 0.5,
            tspin_single: 2.0,
            tspin_double: 5.0,
            tspin_triple: 8.0,
            perfect_clear: 20.0,

            b2b_maintain: 1.0,
            b2b_break: -1.0,
            combo: 0.3,
            wasted_t: -0.5,
            edge_well_bonus: 0.5,

            danger_threshold: 12.0,
            danger_height_mult: 5.0,
            danger_holes_mult: 4.0,
        }
    }
}

impl Default for Weights {
    fn default() -> Self {
        Self::default_aggressive()
    }
}

/// Fast evaluation using precomputed board features.
#[inline]
pub fn evaluate_fast(
    features: &BoardFeatures,
    clear_type: ClearType,
    tspin: TSpinType,
    lines_cleared: u32,
    piece_was_t: bool,
    b2b_before: bool,
    b2b_after: bool,
    combo: u32,
    attack: u32,
    weights: &Weights,
) -> f32 {
    let h = features.max_height as f32;
    let in_danger = h >= weights.danger_threshold;
    let h_mult = if in_danger { weights.danger_height_mult } else { 1.0 };
    let hole_mult = if in_danger { weights.danger_holes_mult } else { 1.0 };

    let mut score = 0.0f32;

    // Board quality
    score += h * weights.max_height * h_mult;
    score += h * h * weights.max_height_sq * h_mult;
    score += features.sum_height as f32 * weights.sum_height;
    score += features.holes as f32 * weights.holes * hole_mult;
    score += features.covered_cells as f32 * weights.covered_cells;
    score += features.bumpiness as f32 * weights.bumpiness;
    score += features.bumpiness_sq as f32 * weights.bumpiness_sq;
    score += features.col_transitions as f32 * weights.col_transitions;
    score += features.row_transitions as f32 * weights.row_transitions;

    // Wells - reward ONE good well, penalize extras
    let well_capped = features.deepest_well.min(4) as f32;
    let excess = (features.total_well_cells - features.deepest_well).max(0) as f32;
    score += well_capped * weights.well_depth;
    score += excess * weights.excess_wells;

    // Edge well bonus: reward well on column 0 or 9
    if features.deepest_well >= 2 && (features.well_column == 0 || features.well_column == 9) {
        score += weights.edge_well_bonus;
    }

    // Clear rewards
    score += match clear_type {
        ClearType::None => 0.0,
        ClearType::Single => weights.single,
        ClearType::Double => weights.double,
        ClearType::Triple => weights.triple,
        ClearType::Tetris => weights.tetris,
        ClearType::TSpinMiniSingle => weights.tspin_mini_single,
        ClearType::TSpinSingle => weights.tspin_single,
        ClearType::TSpinDouble => weights.tspin_double,
        ClearType::TSpinTriple => weights.tspin_triple,
        ClearType::PerfectClear => weights.perfect_clear,
    };

    // B2B chain management
    if lines_cleared > 0 {
        if b2b_after && b2b_before {
            score += weights.b2b_maintain;
        } else if !b2b_after && b2b_before {
            score += weights.b2b_break;
        }
    }

    // Combo
    if combo > 0 {
        score += combo as f32 * weights.combo;
    }

    // Wasted T
    if piece_was_t && tspin == TSpinType::None && lines_cleared == 0 {
        score += weights.wasted_t;
    }

    // Direct attack bonus (reward sending lines)
    score += attack as f32 * 1.5;

    score
}

/// Legacy evaluate function (uses compute_features internally).
pub fn evaluate(
    board: &Board,
    clear_type: ClearType,
    tspin: TSpinType,
    lines_cleared: u32,
    piece_was_t: bool,
    b2b_before: bool,
    b2b_after: bool,
    combo: u32,
    attack: u32,
    _is_pc: bool,
    weights: &Weights,
) -> f32 {
    let features = board.compute_features();
    evaluate_fast(
        &features, clear_type, tspin, lines_cleared, piece_was_t, b2b_before, b2b_after, combo,
        attack, weights,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_board_eval() {
        let board = Board::new();
        let weights = Weights::default();
        let score = evaluate(
            &board, ClearType::None, TSpinType::None, 0, false, false, false, 0, 0, false, &weights,
        );
        assert!(score.abs() < 1.0, "Empty board score: {}", score);
    }

    #[test]
    fn tetris_rewarded_over_single() {
        let board = Board::new();
        let weights = Weights::default();
        let tetris_score = evaluate(
            &board, ClearType::Tetris, TSpinType::None, 4, false, false, false, 1, 4, false, &weights,
        );
        let single_score = evaluate(
            &board, ClearType::Single, TSpinType::None, 1, false, false, false, 1, 0, false, &weights,
        );
        assert!(tetris_score > single_score);
    }

    #[test]
    fn holes_penalized() {
        let mut clean = Board::new();
        clean.rows[0] = 0x3FF;
        clean.rows[1] = 0x3FF;
        clean.rebuild_col_heights();

        let mut holey = Board::new();
        holey.rows[0] = 0x3FE;
        holey.rows[1] = 0x3FF;
        holey.rebuild_col_heights();

        let weights = Weights::default();
        let clean_score = evaluate(
            &clean, ClearType::None, TSpinType::None, 0, false, false, false, 0, 0, false, &weights,
        );
        let holey_score = evaluate(
            &holey, ClearType::None, TSpinType::None, 0, false, false, false, 0, 0, false, &weights,
        );
        assert!(clean_score > holey_score);
    }
}
