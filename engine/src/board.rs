use crate::piece::{PieceType, Rotation};

pub const WIDTH: i8 = 10;
pub const HEIGHT: i8 = 40;
pub const VISIBLE_HEIGHT: i8 = 20;
const FULL_ROW: u16 = (1 << WIDTH) - 1; // 0x3FF

/// 10-wide Tetris board using bitboard rows.
/// Row 0 = bottom, row 39 = top.
/// Each row is a u16 where bit i = column i (0=left, 9=right).
#[derive(Clone)]
pub struct Board {
    pub rows: [u16; HEIGHT as usize],
    /// Height of each column (0 = empty column).
    pub col_heights: [i8; WIDTH as usize],
}

impl Board {
    pub fn new() -> Self {
        Board {
            rows: [0; HEIGHT as usize],
            col_heights: [0; WIDTH as usize],
        }
    }

    #[inline]
    pub fn get(&self, x: i8, y: i8) -> bool {
        if x < 0 || x >= WIDTH || y < 0 || y >= HEIGHT {
            return true; // out-of-bounds = solid
        }
        self.rows[y as usize] & (1 << x) != 0
    }

    #[inline]
    pub fn set(&mut self, x: i8, y: i8) {
        self.rows[y as usize] |= 1 << x;
        if y + 1 > self.col_heights[x as usize] {
            self.col_heights[x as usize] = y + 1;
        }
    }

    /// Check if a piece at (px, py) with given rotation collides with the board or walls.
    #[inline]
    pub fn collides(&self, piece: PieceType, px: i8, py: i8, rot: Rotation) -> bool {
        for &(dx, dy) in piece.cells(rot) {
            let x = px + dx;
            let y = py + dy;
            if x < 0 || x >= WIDTH || y < 0 {
                return true;
            }
            if y < HEIGHT && self.rows[y as usize] & (1 << x) != 0 {
                return true;
            }
        }
        false
    }

    /// Place a piece on the board. Does NOT check for collision.
    pub fn place_piece(&mut self, piece: PieceType, px: i8, py: i8, rot: Rotation) {
        for &(dx, dy) in piece.cells(rot) {
            let x = px + dx;
            let y = py + dy;
            self.set(x, y);
        }
    }

    /// Clear full lines and return the number cleared.
    pub fn clear_lines(&mut self) -> u32 {
        let mut cleared = 0u32;
        let mut write = 0usize;

        for read in 0..HEIGHT as usize {
            if self.rows[read] == FULL_ROW {
                cleared += 1;
            } else {
                self.rows[write] = self.rows[read];
                write += 1;
            }
        }

        // Zero out the top rows
        for i in write..HEIGHT as usize {
            self.rows[i] = 0;
        }

        if cleared > 0 {
            self.rebuild_col_heights();
        }

        cleared
    }

    /// Rebuild column heights from scratch.
    pub fn rebuild_col_heights(&mut self) {
        for col in 0..WIDTH as usize {
            let mask = 1u16 << col;
            let mut h = 0i8;
            for row in (0..HEIGHT as usize).rev() {
                if self.rows[row] & mask != 0 {
                    h = row as i8 + 1;
                    break;
                }
            }
            self.col_heights[col] = h;
        }
    }

    /// Column heights (for debug).
    #[inline]
    pub fn col_heights(&self) -> &[i8; 10] {
        &self.col_heights
    }

    /// Maximum column height.
    #[inline]
    pub fn max_height(&self) -> i8 {
        *self.col_heights.iter().max().unwrap_or(&0)
    }

    /// Sum of all column heights.
    pub fn sum_height(&self) -> i32 {
        self.col_heights.iter().map(|&h| h as i32).sum()
    }

    /// Number of holes (empty cell with any filled cell above it in the same column).
    pub fn hole_count(&self) -> u32 {
        let mut holes = 0u32;
        for col in 0..WIDTH as usize {
            let mask = 1u16 << col;
            let h = self.col_heights[col] as usize;
            for row in 0..h {
                if self.rows[row] & mask == 0 {
                    holes += 1;
                }
            }
        }
        holes
    }

    /// Sum of how deep each hole is (number of filled cells above it).
    pub fn hole_depth_sum(&self) -> u32 {
        let mut depth_sum = 0u32;
        for col in 0..WIDTH as usize {
            let mask = 1u16 << col;
            let h = self.col_heights[col] as usize;
            for row in 0..h {
                if self.rows[row] & mask == 0 {
                    // Count filled cells above this hole
                    let mut depth = 0u32;
                    for above in (row + 1)..h {
                        if self.rows[above] & mask != 0 {
                            depth += 1;
                        }
                    }
                    depth_sum += depth;
                }
            }
        }
        depth_sum
    }

    /// Number of filled cells directly above holes (cells that must be cleared).
    pub fn covered_cells(&self) -> u32 {
        let mut covered = 0u32;
        for col in 0..WIDTH as usize {
            let mask = 1u16 << col;
            let h = self.col_heights[col] as usize;
            let mut found_hole = false;
            for row in 0..h {
                if self.rows[row] & mask == 0 {
                    found_hole = true;
                } else if found_hole {
                    covered += 1;
                }
            }
        }
        covered
    }

    /// Bumpiness: sum of |height[i] - height[i+1]| for adjacent columns.
    pub fn bumpiness(&self) -> i32 {
        let mut bump = 0i32;
        for i in 0..9 {
            bump += (self.col_heights[i] as i32 - self.col_heights[i + 1] as i32).abs();
        }
        bump
    }

    /// Squared bumpiness: sum of (height[i] - height[i+1])^2.
    pub fn bumpiness_sq(&self) -> i32 {
        let mut bump = 0i32;
        for i in 0..9 {
            let diff = self.col_heights[i] as i32 - self.col_heights[i + 1] as i32;
            bump += diff * diff;
        }
        bump
    }

    /// Column transitions: number of filled/empty changes going down each column.
    pub fn col_transitions(&self) -> u32 {
        let mut transitions = 0u32;
        for col in 0..WIDTH as usize {
            let mask = 1u16 << col;
            let h = self.col_heights[col] as usize;
            if h == 0 {
                continue;
            }
            // Top of column to ceiling counts as transition if top is filled
            let mut prev = false; // above the top is empty
            for row in (0..h).rev() {
                let cur = self.rows[row] & mask != 0;
                if cur != prev {
                    transitions += 1;
                }
                prev = cur;
            }
            // Bottom: floor is always solid, so empty at row 0 = transition
            if !prev {
                transitions += 1;
            }
        }
        transitions
    }

    /// Row transitions: number of filled/empty changes going across each row.
    pub fn row_transitions(&self) -> u32 {
        let mut transitions = 0u32;
        let max_h = self.max_height() as usize;
        for row in 0..max_h {
            let r = self.rows[row];
            // Walls on both sides are solid
            let mut prev = true;
            for col in 0..WIDTH as usize {
                let cur = r & (1 << col) != 0;
                if cur != prev {
                    transitions += 1;
                }
                prev = cur;
            }
            // Right wall
            if !prev {
                transitions += 1;
            }
        }
        transitions
    }

    /// Find the deepest well (single-column gap lower than both neighbors).
    /// Returns (well_column, well_depth).
    pub fn deepest_well(&self) -> (i8, i32) {
        let mut best_col = 0i8;
        let mut best_depth = 0i32;

        for col in 0..WIDTH as usize {
            let h = self.col_heights[col] as i32;
            let left_h = if col == 0 { HEIGHT as i32 } else { self.col_heights[col - 1] as i32 };
            let right_h = if col == 9 { HEIGHT as i32 } else { self.col_heights[col + 1] as i32 };
            let min_neighbor = left_h.min(right_h);
            let depth = min_neighbor - h;
            if depth > best_depth {
                best_depth = depth;
                best_col = col as i8;
            }
        }

        (best_col, best_depth)
    }

    /// Total well cells (all wells, not just the deepest).
    pub fn well_cells(&self) -> i32 {
        let mut total = 0i32;
        for col in 0..WIDTH as usize {
            let h = self.col_heights[col] as i32;
            let left_h = if col == 0 { HEIGHT as i32 } else { self.col_heights[col - 1] as i32 };
            let right_h = if col == 9 { HEIGHT as i32 } else { self.col_heights[col + 1] as i32 };
            let min_neighbor = left_h.min(right_h);
            let depth = min_neighbor - h;
            if depth > 0 {
                total += depth;
            }
        }
        total
    }

    /// True if the board is completely empty.
    pub fn is_empty(&self) -> bool {
        self.rows.iter().all(|&r| r == 0)
    }

    /// Add a garbage line at the bottom with a hole at the given column.
    pub fn add_garbage(&mut self, hole_col: i8) {
        // Shift all rows up by 1
        for row in (1..HEIGHT as usize).rev() {
            self.rows[row] = self.rows[row - 1];
        }
        // Bottom row = full except hole
        self.rows[0] = FULL_ROW & !(1 << hole_col);
        self.rebuild_col_heights();
    }

    /// Hard drop: find the lowest y where the piece doesn't collide.
    pub fn sonic_drop(&self, piece: PieceType, px: i8, py: i8, rot: Rotation) -> i8 {
        let mut y = py;
        while !self.collides(piece, px, y - 1, rot) {
            y -= 1;
        }
        y
    }

    /// Compute all board features in a single pass for maximum speed.
    pub fn compute_features(&self) -> BoardFeatures {
        let mut f = BoardFeatures::default();
        let max_h = self.max_height();
        f.max_height = max_h as i32;

        if max_h == 0 {
            return f;
        }

        let mh = max_h as usize;

        // Column-based features: holes, bumpiness, wells, col_transitions
        for col in 0..WIDTH as usize {
            let mask = 1u16 << col;
            let h = self.col_heights[col] as i32;
            f.sum_height += h;

            // Holes and covered cells
            let mut found_filled = false;
            let mut col_holes = 0i32;
            for row in (0..self.col_heights[col] as usize).rev() {
                let filled = self.rows[row] & mask != 0;
                if filled {
                    found_filled = true;
                    if col_holes > 0 {
                        f.covered_cells += 1;
                    }
                } else if found_filled {
                    col_holes += 1;
                }
            }
            f.holes += col_holes;

            // Column transitions
            if h > 0 {
                let mut prev = false;
                for row in (0..h as usize).rev() {
                    let cur = self.rows[row] & mask != 0;
                    if cur != prev {
                        f.col_transitions += 1;
                    }
                    prev = cur;
                }
                if !prev {
                    f.col_transitions += 1;
                }
            }

            // Bumpiness
            if col > 0 {
                let diff = self.col_heights[col] as i32 - self.col_heights[col - 1] as i32;
                f.bumpiness += diff.abs();
                f.bumpiness_sq += diff * diff;
            }

            // Wells
            let left_h = if col == 0 { HEIGHT as i32 } else { self.col_heights[col - 1] as i32 };
            let right_h = if col == 9 { HEIGHT as i32 } else { self.col_heights[col + 1] as i32 };
            let well = left_h.min(right_h) - h;
            if well > 0 {
                f.total_well_cells += well;
                if well > f.deepest_well {
                    f.deepest_well = well;
                    f.well_column = col as i8;
                }
            }
        }

        // Row transitions
        for row in 0..mh {
            let r = self.rows[row];
            // XOR with walls-included version to count transitions fast
            // Shift row right by 1 and compare; also check left wall and right wall
            let with_walls = r | (1 << WIDTH); // right wall
            let shifted = (r << 1) | 1; // left wall
            let transitions = with_walls ^ shifted;
            f.row_transitions += transitions.count_ones() as i32;
        }

        f
    }
}

/// All board features computed in a single pass.
#[derive(Debug, Clone, Default)]
pub struct BoardFeatures {
    pub max_height: i32,
    pub sum_height: i32,
    pub holes: i32,
    pub covered_cells: i32,
    pub bumpiness: i32,
    pub bumpiness_sq: i32,
    pub col_transitions: i32,
    pub row_transitions: i32,
    pub deepest_well: i32,
    pub well_column: i8,
    pub total_well_cells: i32,
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Board {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f)?;
        for row in (0..VISIBLE_HEIGHT as usize).rev() {
            write!(f, "|")?;
            for col in 0..WIDTH as usize {
                if self.rows[row] & (1 << col) != 0 {
                    write!(f, "X")?;
                } else {
                    write!(f, ".")?;
                }
            }
            writeln!(f, "|")?;
        }
        write!(f, "+{}+", "-".repeat(WIDTH as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_board() {
        let b = Board::new();
        assert_eq!(b.max_height(), 0);
        assert_eq!(b.hole_count(), 0);
        assert!(b.is_empty());
    }

    #[test]
    fn place_and_clear() {
        let mut b = Board::new();
        // Fill bottom row completely
        b.rows[0] = FULL_ROW;
        b.rebuild_col_heights();
        assert_eq!(b.max_height(), 1);

        let cleared = b.clear_lines();
        assert_eq!(cleared, 1);
        assert!(b.is_empty());
    }

    #[test]
    fn collision_walls() {
        let b = Board::new();
        // I piece at far left should collide if pushed further left
        assert!(b.collides(PieceType::I, -1, 0, Rotation::State0));
        // I piece centered should not collide
        assert!(!b.collides(PieceType::I, 4, 0, Rotation::State0));
    }

    #[test]
    fn sonic_drop_empty() {
        let b = Board::new();
        let y = b.sonic_drop(PieceType::T, 4, 20, Rotation::State0);
        assert_eq!(y, 0);
    }

    #[test]
    fn hole_counting() {
        let mut b = Board::new();
        // Column 0: filled at row 1, empty at row 0 = 1 hole
        b.rows[1] = 1; // bit 0 set
        b.rebuild_col_heights();
        assert_eq!(b.hole_count(), 1);
    }

    #[test]
    fn garbage_line() {
        let mut b = Board::new();
        b.add_garbage(5);
        assert_eq!(b.rows[0], FULL_ROW & !(1 << 5));
        assert!(!b.get(5, 0)); // hole
        assert!(b.get(4, 0));  // filled
    }

    #[test]
    fn bumpiness_flat() {
        let mut b = Board::new();
        for col in 0..10 {
            b.col_heights[col] = 3;
        }
        assert_eq!(b.bumpiness(), 0);
    }
}
