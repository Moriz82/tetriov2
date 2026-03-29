use crate::board::Board;
use crate::piece::{PieceType, Rotation, kicks_ccw, kicks_cw};

/// A possible piece placement on the board.
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub piece: PieceType,
    pub x: i8,
    pub y: i8,
    pub rot: Rotation,
    pub tspin: TSpinType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TSpinType {
    None,
    Mini,
    Full,
}

/// BFS state for move generation.
#[derive(Clone, Copy)]
struct State {
    x: i8,
    y: i8,
    rot: Rotation,
    last_was_rotation: bool,
}

/// Visited set using a flat bitset.
/// Dimensions: x in [-2, 12] (15 values), y in [-2, 42] (45 values),
/// rot in [0, 3] (4 values), last_rot in [0, 1] (2 values).
/// Total: 15 * 45 * 4 * 2 = 5400 bits = 675 bytes.
const X_OFFSET: i8 = 2;
const X_RANGE: usize = 15;
const Y_OFFSET: i8 = 2;
const Y_RANGE: usize = 45;
const ROT_RANGE: usize = 4;
const LAST_ROT_RANGE: usize = 2;
const VISITED_BITS: usize = X_RANGE * Y_RANGE * ROT_RANGE * LAST_ROT_RANGE;
const VISITED_WORDS: usize = (VISITED_BITS + 63) / 64;

struct VisitedSet {
    bits: [u64; VISITED_WORDS],
}

impl VisitedSet {
    fn new() -> Self {
        VisitedSet {
            bits: [0; VISITED_WORDS],
        }
    }

    #[inline]
    fn index(x: i8, y: i8, rot: Rotation, last_rot: bool) -> usize {
        let xi = (x + X_OFFSET) as usize;
        let yi = (y + Y_OFFSET) as usize;
        let ri = rot as usize;
        let li = last_rot as usize;
        li * (X_RANGE * Y_RANGE * ROT_RANGE) + ri * (X_RANGE * Y_RANGE) + yi * X_RANGE + xi
    }

    #[inline]
    fn in_range(x: i8, y: i8) -> bool {
        let xi = x + X_OFFSET;
        let yi = y + Y_OFFSET;
        xi >= 0 && (xi as usize) < X_RANGE && yi >= 0 && (yi as usize) < Y_RANGE
    }

    #[inline]
    fn test_and_set(&mut self, x: i8, y: i8, rot: Rotation, last_rot: bool) -> bool {
        if !Self::in_range(x, y) {
            return true; // treat out-of-range as visited
        }
        let idx = Self::index(x, y, rot, last_rot);
        let word = idx / 64;
        let bit = idx % 64;
        let mask = 1u64 << bit;
        if self.bits[word] & mask != 0 {
            return true;
        }
        self.bits[word] |= mask;
        false
    }
}

/// Find all possible placements for a piece on the board.
/// Uses BFS from the spawn position to find all reachable positions,
/// including tucks and T-spins.
pub fn generate_placements(board: &Board, piece: PieceType) -> Vec<Placement> {
    let spawn_x = piece.spawn_x();
    let spawn_y = piece.spawn_y();

    // If spawn position collides, game is over - return empty
    if board.collides(piece, spawn_x, spawn_y, Rotation::State0) {
        return Vec::new();
    }

    let mut visited = VisitedSet::new();
    let mut queue = Vec::with_capacity(512);
    let mut placements = Vec::with_capacity(64);

    // Deduplicate final positions: [rot][x] -> best y and tspin info
    // We track seen positions separately to avoid duplicate placements.
    let mut seen_final: Vec<(i8, i8, Rotation, TSpinType)> = Vec::with_capacity(64);

    let initial = State {
        x: spawn_x,
        y: spawn_y,
        rot: Rotation::State0,
        last_was_rotation: false,
    };
    visited.test_and_set(spawn_x, spawn_y, Rotation::State0, false);
    queue.push(initial);

    let mut head = 0;
    while head < queue.len() {
        let state = queue[head];
        head += 1;

        // Check if this position is on the ground (can't move down)
        let on_ground = board.collides(piece, state.x, state.y - 1, state.rot);

        if on_ground {
            // This is a valid final position - add as placement
            let tspin = if piece == PieceType::T && state.last_was_rotation {
                detect_tspin(board, state.x, state.y, state.rot)
            } else {
                TSpinType::None
            };

            // Deduplicate: same (x, y, rot) should only appear once.
            // Prefer the one with the higher tspin type.
            let key = (state.x, state.y, state.rot);
            let existing = seen_final
                .iter()
                .position(|&(ex, ey, er, _)| ex == key.0 && ey == key.1 && er == key.2);

            if let Some(idx) = existing {
                // Keep the one with better tspin
                if tspin_rank(tspin) > tspin_rank(seen_final[idx].3) {
                    seen_final[idx].3 = tspin;
                }
            } else {
                seen_final.push((state.x, state.y, state.rot, tspin));
            }
        }

        // Try movements: left, right, soft drop
        let moves: [(i8, i8); 3] = [(-1, 0), (1, 0), (0, -1)];
        for &(dx, dy) in &moves {
            let nx = state.x + dx;
            let ny = state.y + dy;
            let nrot = state.rot;
            let last_rot = false;

            if !board.collides(piece, nx, ny, nrot)
                && !visited.test_and_set(nx, ny, nrot, last_rot)
            {
                queue.push(State {
                    x: nx,
                    y: ny,
                    rot: nrot,
                    last_was_rotation: last_rot,
                });
            }
        }

        // Try rotations: CW, CCW
        for (get_kicks, target_rot) in [
            (kicks_cw as fn(PieceType, Rotation) -> &'static [(i8, i8)], state.rot.cw()),
            (kicks_ccw as fn(PieceType, Rotation) -> &'static [(i8, i8)], state.rot.ccw()),
        ] {
            let kicks = get_kicks(piece, state.rot);
            for &(kx, ky) in kicks {
                let nx = state.x + kx;
                let ny = state.y + ky;
                if !board.collides(piece, nx, ny, target_rot) {
                    let last_rot = true;
                    if !visited.test_and_set(nx, ny, target_rot, last_rot) {
                        queue.push(State {
                            x: nx,
                            y: ny,
                            rot: target_rot,
                            last_was_rotation: last_rot,
                        });
                    }
                    break;
                }
            }
        }

        // Also try reaching ground positions via soft drop that aren't "on ground"
        // This is handled by the (0, -1) move above.
    }

    // Convert seen_final to placements
    for &(x, y, rot, tspin) in &seen_final {
        placements.push(Placement {
            piece,
            x,
            y,
            rot,
            tspin,
        });
    }

    placements
}

/// Generate placements that include a hard drop from any reachable position.
/// This extends the basic BFS by also considering hard drops from non-ground positions.
pub fn generate_placements_with_drops(board: &Board, piece: PieceType) -> Vec<Placement> {
    let spawn_x = piece.spawn_x();
    let spawn_y = piece.spawn_y();

    if board.collides(piece, spawn_x, spawn_y, Rotation::State0) {
        return Vec::new();
    }

    let mut visited = VisitedSet::new();
    let mut queue = Vec::with_capacity(512);
    // Use a separate set for final positions
    let mut final_set: Vec<(i8, i8, u8, TSpinType)> = Vec::with_capacity(64);

    let initial = State {
        x: spawn_x,
        y: spawn_y,
        rot: Rotation::State0,
        last_was_rotation: false,
    };
    visited.test_and_set(spawn_x, spawn_y, Rotation::State0, false);
    queue.push(initial);

    let mut head = 0;
    while head < queue.len() {
        let state = queue[head];
        head += 1;

        // Hard drop from current position
        let drop_y = board.sonic_drop(piece, state.x, state.y, state.rot);
        let on_ground_after_drop = drop_y == state.y;

        let tspin = if piece == PieceType::T && state.last_was_rotation && on_ground_after_drop {
            detect_tspin(board, state.x, state.y, state.rot)
        } else {
            TSpinType::None
        };

        // Record this final position
        let key = (state.x, drop_y, state.rot as u8);
        let existing = final_set
            .iter()
            .position(|&(ex, ey, er, _)| ex == key.0 && ey == key.1 && er == key.2);

        if let Some(idx) = existing {
            if tspin_rank(tspin) > tspin_rank(final_set[idx].3) {
                final_set[idx].3 = tspin;
            }
        } else {
            final_set.push((key.0, key.1, key.2, tspin));
        }

        // Try movements: left, right, soft drop
        for &(dx, dy) in &[(-1i8, 0i8), (1, 0), (0, -1)] {
            let nx = state.x + dx;
            let ny = state.y + dy;
            if !board.collides(piece, nx, ny, state.rot)
                && !visited.test_and_set(nx, ny, state.rot, false)
            {
                queue.push(State {
                    x: nx,
                    y: ny,
                    rot: state.rot,
                    last_was_rotation: false,
                });
            }
        }

        // Try rotations
        for (get_kicks, target_rot) in [
            (kicks_cw as fn(PieceType, Rotation) -> &'static [(i8, i8)], state.rot.cw()),
            (kicks_ccw as fn(PieceType, Rotation) -> &'static [(i8, i8)], state.rot.ccw()),
        ] {
            let kicks = get_kicks(piece, state.rot);
            for &(kx, ky) in kicks {
                let nx = state.x + kx;
                let ny = state.y + ky;
                if !board.collides(piece, nx, ny, target_rot) {
                    if !visited.test_and_set(nx, ny, target_rot, true) {
                        queue.push(State {
                            x: nx,
                            y: ny,
                            rot: target_rot,
                            last_was_rotation: true,
                        });
                    }
                    break;
                }
            }
        }
    }

    final_set
        .into_iter()
        .map(|(x, y, rot_u8, tspin)| {
            let rot = match rot_u8 {
                0 => Rotation::State0,
                1 => Rotation::StateR,
                2 => Rotation::State2,
                _ => Rotation::StateL,
            };
            Placement {
                piece,
                x,
                y,
                rot,
                tspin,
            }
        })
        .collect()
}

/// Detect T-spin using the 3-corner rule.
/// Must only be called for T pieces at their final position.
fn detect_tspin(board: &Board, px: i8, py: i8, rot: Rotation) -> TSpinType {
    // The 4 diagonal corners of the T piece center
    let corners = [
        board.get(px - 1, py + 1), // top-left
        board.get(px + 1, py + 1), // top-right
        board.get(px - 1, py - 1), // bottom-left
        board.get(px + 1, py - 1), // bottom-right
    ];

    let filled = corners.iter().filter(|&&c| c).count();

    if filled < 3 {
        return TSpinType::None;
    }

    // 3 or 4 corners filled.
    // Full T-spin: the two corners on the "front" (pointing) side are both filled.
    // Mini T-spin: otherwise.
    let front_filled = match rot {
        Rotation::State0 => corners[0] && corners[1], // pointing up
        Rotation::StateR => corners[1] && corners[3], // pointing right
        Rotation::State2 => corners[2] && corners[3], // pointing down
        Rotation::StateL => corners[0] && corners[2], // pointing left
    };

    if front_filled {
        TSpinType::Full
    } else {
        TSpinType::Mini
    }
}

/// Fast column-drop placement generator for use in deeper search levels.
/// Only tries each rotation + column combination with a straight drop.
/// Much faster than full BFS but misses tucks and T-spins.
pub fn generate_placements_fast(board: &Board, piece: PieceType) -> Vec<Placement> {
    let mut placements = Vec::with_capacity(40);
    let rotations = if piece == PieceType::O {
        &[Rotation::State0][..]
    } else {
        &[Rotation::State0, Rotation::StateR, Rotation::State2, Rotation::StateL][..]
    };

    for &rot in rotations {
        let cells = piece.cells(rot);

        // Find valid x range for this rotation
        let min_dx = cells.iter().map(|&(dx, _)| dx).min().unwrap();
        let max_dx = cells.iter().map(|&(dx, _)| dx).max().unwrap();
        let x_min = -min_dx;
        let x_max = 9 - max_dx;

        for x in x_min..=x_max {
            // Start from a high position and drop
            let start_y = board.max_height() + 4;
            if board.collides(piece, x, start_y, rot) {
                continue;
            }
            let y = board.sonic_drop(piece, x, start_y, rot);
            placements.push(Placement {
                piece,
                x,
                y,
                rot,
                tspin: TSpinType::None,
            });
        }
    }

    placements
}

fn tspin_rank(t: TSpinType) -> u8 {
    match t {
        TSpinType::None => 0,
        TSpinType::Mini => 1,
        TSpinType::Full => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placements_on_empty_board() {
        let board = Board::new();
        for piece in PieceType::ALL {
            let placements = generate_placements_with_drops(&board, piece);
            assert!(!placements.is_empty(), "No placements for {:?}", piece);
        }
    }

    #[test]
    fn i_piece_placement_count() {
        let board = Board::new();
        let placements = generate_placements_with_drops(&board, PieceType::I);
        // I piece: 10 columns for horizontal (some may be restricted) +
        // columns for vertical. Should be a reasonable number.
        assert!(placements.len() >= 17, "Too few I placements: {}", placements.len());
    }

    #[test]
    fn no_placements_when_topped_out() {
        let mut board = Board::new();
        // Fill the board to the top
        for row in 0..22 {
            board.rows[row] = 0x3FF;
        }
        board.rebuild_col_heights();

        let placements = generate_placements_with_drops(&board, PieceType::T);
        assert!(placements.is_empty());
    }

    #[test]
    fn tspin_detection() {
        // Set up a T-spin double slot:
        //  |XXX.XX.XXX|
        //  |XXXX.XXXXX|
        let mut board = Board::new();
        board.rows[0] = 0b1111011111; // row 0: all except col 5... wait, let me think
        // For a T-spin double, we need:
        // Row 0: full except one gap
        // Row 1: full except two gaps forming a T-slot

        // T-spin slot at column 4:
        // Row 0: XXXX.XXXXX (hole at col 4)
        // Row 1: XXX..XXXXX (holes at col 3,4)
        // Row 2: XXXXXXXXXX (full, to create corner)
        // Wait, we need corners to be filled for the T-spin.

        // Simpler test: create a basic T-spin setup
        // T piece pointing down (State2) at position (4, 1)
        // Corners at (3,2), (5,2), (3,0), (5,0)
        board.rows[0] = 0b1111111111; // full
        board.rows[1] = 0b1111100111; // gap at cols 3,4 (we need space for the T)

        // Actually, the T in state2 has cells: (-1,0), (0,0), (1,0), (0,-1)
        // At position (4, 1): cells at (3,1), (4,1), (5,1), (4,0)
        // We need cols 3,4,5 at row 1 to be empty, and col 4 at row 0 to be empty
        board.rows[0] = 0b1111101111; // gap at col 4
        board.rows[1] = 0b1111000111; // gap at cols 3,4,5
        board.rows[2] = 0b1111111111; // full (to create top corners)

        board.rebuild_col_heights();

        // Check corners for T at (4, 1) in State2:
        // Corners: (3,2), (5,2), (3,0), (5,0)
        let tspin = detect_tspin(&board, 4, 1, Rotation::State2);
        assert_eq!(tspin, TSpinType::Full);
    }
}
