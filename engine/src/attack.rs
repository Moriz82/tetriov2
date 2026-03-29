use crate::movegen::TSpinType;

/// TETR.IO attack table - lines sent for each clear type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearType {
    None,
    Single,
    Double,
    Triple,
    Tetris,
    TSpinMiniSingle,
    TSpinSingle,
    TSpinDouble,
    TSpinTriple,
    PerfectClear,
}

/// Calculate the clear type from the number of lines cleared and T-spin status.
pub fn classify_clear(lines: u32, tspin: TSpinType) -> ClearType {
    match (lines, tspin) {
        (0, _) => ClearType::None,
        (1, TSpinType::None) => ClearType::Single,
        (2, TSpinType::None) => ClearType::Double,
        (3, TSpinType::None) => ClearType::Triple,
        (4, TSpinType::None) => ClearType::Tetris,
        (1, TSpinType::Mini) => ClearType::TSpinMiniSingle,
        (1, TSpinType::Full) => ClearType::TSpinSingle,
        (2, TSpinType::Full | TSpinType::Mini) => ClearType::TSpinDouble,
        (3, TSpinType::Full | TSpinType::Mini) => ClearType::TSpinTriple,
        _ => ClearType::None,
    }
}

/// Base attack lines for each clear type (before B2B and combo).
pub fn base_attack(clear: ClearType) -> u32 {
    match clear {
        ClearType::None => 0,
        ClearType::Single => 0,
        ClearType::Double => 1,
        ClearType::Triple => 2,
        ClearType::Tetris => 4,
        ClearType::TSpinMiniSingle => 0,
        ClearType::TSpinSingle => 2,
        ClearType::TSpinDouble => 4,
        ClearType::TSpinTriple => 6,
        ClearType::PerfectClear => 10,
    }
}

/// Whether this clear type maintains back-to-back chain.
pub fn is_b2b_clear(clear: ClearType) -> bool {
    matches!(
        clear,
        ClearType::Tetris
            | ClearType::TSpinMiniSingle
            | ClearType::TSpinSingle
            | ClearType::TSpinDouble
            | ClearType::TSpinTriple
    )
}

/// Whether this clear type breaks back-to-back chain.
pub fn breaks_b2b(clear: ClearType) -> bool {
    matches!(
        clear,
        ClearType::Single | ClearType::Double | ClearType::Triple
    )
}

/// Combo attack bonus (TETR.IO combo table).
pub fn combo_attack(combo: u32) -> u32 {
    match combo {
        0 => 0,
        1 => 0,
        2 => 1,
        3 => 1,
        4 => 2,
        5 => 2,
        6 => 3,
        7 => 3,
        8 => 4,
        9 => 4,
        10 => 4,
        11 => 5,
        _ => 5,
    }
}

/// Calculate total attack for a clear.
pub fn calculate_attack(clear: ClearType, b2b_active: bool, combo: u32, is_pc: bool) -> u32 {
    if is_pc {
        return 10;
    }

    let mut attack = base_attack(clear);

    // B2B bonus
    if b2b_active && is_b2b_clear(clear) {
        attack += 1;
    }

    // Combo bonus
    attack += combo_attack(combo);

    attack
}
