/// Piece types, rotation states, cell definitions, and SRS kick tables.
///
/// Coordinate convention:
///   x (column): 0 = left, 9 = right
///   y (row):    0 = bottom, 39 = top
///   Cell offsets (dx, dy) are relative to the piece position on the board.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PieceType {
    I = 0,
    J = 1,
    L = 2,
    O = 3,
    S = 4,
    T = 5,
    Z = 6,
}

impl PieceType {
    pub const ALL: [PieceType; 7] = [
        PieceType::I,
        PieceType::J,
        PieceType::L,
        PieceType::O,
        PieceType::S,
        PieceType::T,
        PieceType::Z,
    ];

    #[inline]
    pub fn cells(self, rot: Rotation) -> &'static [(i8, i8); 4] {
        &PIECE_CELLS[self as usize][rot as usize]
    }

    #[inline]
    pub fn spawn_x(self) -> i8 {
        4
    }

    #[inline]
    pub fn spawn_y(self) -> i8 {
        20
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Rotation {
    State0 = 0,
    StateR = 1,
    State2 = 2,
    StateL = 3,
}

impl Rotation {
    #[inline]
    pub fn cw(self) -> Rotation {
        match self {
            Rotation::State0 => Rotation::StateR,
            Rotation::StateR => Rotation::State2,
            Rotation::State2 => Rotation::StateL,
            Rotation::StateL => Rotation::State0,
        }
    }

    #[inline]
    pub fn ccw(self) -> Rotation {
        match self {
            Rotation::State0 => Rotation::StateL,
            Rotation::StateR => Rotation::State0,
            Rotation::State2 => Rotation::StateR,
            Rotation::StateL => Rotation::State2,
        }
    }
}

// ---------------------------------------------------------------------------
// Cell offset tables: PIECE_CELLS[piece_type][rotation] = [(dx, dy); 4]
//
// Derived from the SRS bounding-box definitions (y-up convention).
// Reference point for 3x3 pieces (J,L,S,T,Z) = center cell of the box.
// Reference point for I = bounding box position (col=1, row=1 from top).
// Reference point for O = bounding box position (col=1, row=1 from top).
// ---------------------------------------------------------------------------

const PIECE_CELLS: [[[(i8, i8); 4]; 4]; 7] = [
    // I
    [
        [(-1, 0), (0, 0), (1, 0), (2, 0)],
        [(1, 1), (1, 0), (1, -1), (1, -2)],
        [(-1, -1), (0, -1), (1, -1), (2, -1)],
        [(0, 1), (0, 0), (0, -1), (0, -2)],
    ],
    // J
    [
        [(-1, 1), (-1, 0), (0, 0), (1, 0)],
        [(0, 1), (1, 1), (0, 0), (0, -1)],
        [(-1, 0), (0, 0), (1, 0), (1, -1)],
        [(0, 1), (0, 0), (-1, -1), (0, -1)],
    ],
    // L
    [
        [(1, 1), (-1, 0), (0, 0), (1, 0)],
        [(0, 1), (0, 0), (0, -1), (1, -1)],
        [(-1, 0), (0, 0), (1, 0), (-1, -1)],
        [(-1, 1), (0, 1), (0, 0), (0, -1)],
    ],
    // O (same cells all rotations; SRS offsets handle position)
    [
        [(0, 1), (1, 1), (0, 0), (1, 0)],
        [(0, 1), (1, 1), (0, 0), (1, 0)],
        [(0, 1), (1, 1), (0, 0), (1, 0)],
        [(0, 1), (1, 1), (0, 0), (1, 0)],
    ],
    // S
    [
        [(0, 1), (1, 1), (-1, 0), (0, 0)],
        [(0, 1), (0, 0), (1, 0), (1, -1)],
        [(0, 0), (1, 0), (-1, -1), (0, -1)],
        [(-1, 1), (-1, 0), (0, 0), (0, -1)],
    ],
    // T
    [
        [(0, 1), (-1, 0), (0, 0), (1, 0)],
        [(0, 1), (0, 0), (1, 0), (0, -1)],
        [(-1, 0), (0, 0), (1, 0), (0, -1)],
        [(0, 1), (-1, 0), (0, 0), (0, -1)],
    ],
    // Z
    [
        [(-1, 1), (0, 1), (0, 0), (1, 0)],
        [(1, 1), (0, 0), (1, 0), (0, -1)],
        [(-1, 0), (0, 0), (0, -1), (1, -1)],
        [(0, 1), (-1, 0), (0, 0), (-1, -1)],
    ],
];

// ---------------------------------------------------------------------------
// SRS kick tables.
//
// All values in (dx, dy) with x-right, y-up.
// Derived from SRS offset data: kick = offset_from[i] - offset_to[i].
// ---------------------------------------------------------------------------

pub fn kicks_cw(piece: PieceType, from: Rotation) -> &'static [(i8, i8)] {
    match piece {
        PieceType::O => &O_KICKS_CW[from as usize],
        PieceType::I => &I_KICKS_CW[from as usize],
        _ => &JLSTZ_KICKS_CW[from as usize],
    }
}

pub fn kicks_ccw(piece: PieceType, from: Rotation) -> &'static [(i8, i8)] {
    match piece {
        PieceType::O => &O_KICKS_CCW[from as usize],
        PieceType::I => &I_KICKS_CCW[from as usize],
        _ => &JLSTZ_KICKS_CCW[from as usize],
    }
}

const JLSTZ_KICKS_CW: [[(i8, i8); 5]; 4] = [
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],   // 0->R
    [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],      // R->2
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],       // 2->L
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],   // L->0
];

const JLSTZ_KICKS_CCW: [[(i8, i8); 5]; 4] = [
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],       // 0->L
    [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],      // R->0
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],    // 2->R
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],   // L->2
];

// I piece kicks derived from I offset data (y-up).
const I_KICKS_CW: [[(i8, i8); 5]; 4] = [
    [(1, 0), (-1, 0), (2, 0), (-1, 1), (2, -2)],         // 0->R
    [(0, 1), (-1, 1), (2, 1), (-1, -1), (2, 2)],          // R->2
    [(-1, 0), (1, 0), (-2, 0), (1, -1), (-2, 2)],         // 2->L
    [(0, -1), (1, -1), (-2, -1), (1, 1), (-2, -2)],       // L->0
];

const I_KICKS_CCW: [[(i8, i8); 5]; 4] = [
    [(0, 1), (-1, 1), (2, 1), (-1, -1), (2, 2)],         // 0->L
    [(-1, 0), (1, 0), (-2, 0), (1, -1), (-2, 2)],        // R->0
    [(0, -1), (1, -1), (-2, -1), (1, 1), (-2, -2)],      // 2->R
    [(1, 0), (-1, 0), (2, 0), (-1, 1), (2, -2)],         // L->2
];

// O piece: single kick test per rotation (position shift from offset data).
const O_KICKS_CW: [[(i8, i8); 5]; 4] = [
    [(0, -1), (0, 0), (0, 0), (0, 0), (0, 0)],  // 0->R
    [(1, 0), (0, 0), (0, 0), (0, 0), (0, 0)],   // R->2
    [(0, 1), (0, 0), (0, 0), (0, 0), (0, 0)],   // 2->L
    [(-1, 0), (0, 0), (0, 0), (0, 0), (0, 0)],  // L->0
];

const O_KICKS_CCW: [[(i8, i8); 5]; 4] = [
    [(1, 0), (0, 0), (0, 0), (0, 0), (0, 0)],   // 0->L
    [(0, 1), (0, 0), (0, 0), (0, 0), (0, 0)],   // R->0
    [(-1, 0), (0, 0), (0, 0), (0, 0), (0, 0)],  // 2->R
    [(0, -1), (0, 0), (0, 0), (0, 0), (0, 0)],  // L->2
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_cycle() {
        let r = Rotation::State0;
        assert_eq!(r.cw().cw().cw().cw(), Rotation::State0);
        assert_eq!(r.ccw().ccw().ccw().ccw(), Rotation::State0);
        assert_eq!(r.cw().ccw(), Rotation::State0);
    }

    #[test]
    fn i_piece_state0_horizontal() {
        let cells = PieceType::I.cells(Rotation::State0);
        assert!(cells.iter().all(|&(_, dy)| dy == 0));
        let mut xs: Vec<i8> = cells.iter().map(|&(dx, _)| dx).collect();
        xs.sort();
        assert_eq!(xs, vec![-1, 0, 1, 2]);
    }

    #[test]
    fn t_piece_state0_shape() {
        let cells = PieceType::T.cells(Rotation::State0);
        assert_eq!(cells[0], (0, 1));
        assert_eq!(cells[1], (-1, 0));
        assert_eq!(cells[2], (0, 0));
        assert_eq!(cells[3], (1, 0));
    }

    #[test]
    fn each_piece_has_4_cells() {
        for piece in PieceType::ALL {
            for rot in [Rotation::State0, Rotation::StateR, Rotation::State2, Rotation::StateL] {
                assert_eq!(piece.cells(rot).len(), 4);
            }
        }
    }

    #[test]
    fn kick_tables_have_5_entries() {
        for piece in PieceType::ALL {
            for rot in [Rotation::State0, Rotation::StateR, Rotation::State2, Rotation::StateL] {
                assert_eq!(kicks_cw(piece, rot).len(), 5);
                assert_eq!(kicks_ccw(piece, rot).len(), 5);
            }
        }
    }
}
