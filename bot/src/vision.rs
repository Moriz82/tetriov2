//! Board state extraction from screenshot pixels via color detection

use tetrio_engine::PieceType;

/// Brightness threshold for a cell to be considered filled.
/// TETR.IO empty cells are ~45-78 RGB sum. Locked pieces ~300+.
/// Use 150 — well above empty cells, catches dimmer locked pieces.
const FILL_THRESHOLD: u32 = 150;

#[derive(Debug, Clone)]
pub struct Calibration {
    // Screen-absolute coordinates (used for grim capture region)
    pub capture_x: u32,
    pub capture_y: u32,
    pub capture_w: u32,
    pub capture_h: u32,
    // All coordinates below are RELATIVE to capture region
    pub board_x: u32,
    pub board_y: u32,
    pub cell_w: f64,
    pub cell_h: f64,
}

#[derive(Debug, Clone)]
pub struct VisionState {
    /// 20 rows x 10 cols. board[0] = top visible row, board[19] = bottom.
    pub board: [[bool; 10]; 20],
    pub current_piece: Option<PieceType>,
    pub hold: Option<PieceType>,
    pub queue: Vec<PieceType>,
}

/// Let user select the board area.
/// Linux: slurp (Wayland region selector)
/// macOS: screencapture -i (interactive selection, writes coords to temp file)
pub fn manual_calibrate() -> Option<Calibration> {
    let (sel_x, sel_y, sel_w, sel_h) = platform_select_region()?;

    let ratio = sel_w as f64 / sel_h as f64;
    let cell_size = if ratio < 0.7 {
        sel_w as f64 / 10.0
    } else {
        sel_h as f64 / 20.0
    };

    let cell_h = cell_size;
    // Cap cell_w so column 9 samples don't extend into the attack meter.
    // The attack meter sits flush against the board's right edge.
    let width_derived = sel_w as f64 / 10.0;
    let cell_w = cell_size.min(width_derived) * 0.98; // 2% safety margin
    let actual_w = cell_w * 10.0;
    let actual_h = cell_h * 20.0;

    // Board position in screen-absolute coords
    let abs_board_x = sel_x + ((sel_w as f64 - actual_w) / 2.0).max(0.0) as u32;
    let abs_board_y = sel_y + ((sel_h as f64 - actual_h) / 2.0).max(0.0) as u32;

    // Compute capture region: generous area around the board to include hold + queue
    let margin_x = (cell_size * 6.0) as u32; // enough for hold/queue
    let margin_y = (cell_size * 3.0) as u32;
    let cap_x = abs_board_x.saturating_sub(margin_x);
    let cap_y = abs_board_y.saturating_sub(margin_y);
    let cap_right = abs_board_x + actual_w as u32 + margin_x;
    let cap_bottom = abs_board_y + actual_h as u32 + margin_y;
    let cap_w = cap_right - cap_x;
    let cap_h = cap_bottom - cap_y;

    let cal = Calibration {
        capture_x: cap_x,
        capture_y: cap_y,
        capture_w: cap_w,
        capture_h: cap_h,
        board_x: abs_board_x - cap_x,
        board_y: abs_board_y - cap_y,
        cell_w, cell_h,
    };

    eprintln!("[Cal] Selection: {}x{} at ({},{})", sel_w, sel_h, sel_x, sel_y);
    eprintln!("[Cal] Board:     {:.0}x{:.0} at ({},{}) cell={:.1}px",
        actual_w, actual_h, abs_board_x, abs_board_y, cell_size);
    eprintln!("[Cal] Capture:   {}x{} at ({},{}) — region only, fast!",
        cap_w, cap_h, cap_x, cap_y);
    Some(cal)
}

pub fn extract_state(pixels: &[u8], width: u32, cal: &Calibration) -> VisionState {
    let mut raw_board = [[false; 10]; 20];

    // Sample 3x3 grid per cell. Use color detection (not just brightness)
    // to reject ghost pieces (semi-transparent, low saturation).
    for row in 0..20 {
        for col in 0..10 {
            let cx = cal.board_x as f64 + (col as f64 + 0.5) * cal.cell_w;
            let cy = cal.board_y as f64 + (row as f64 + 0.5) * cal.cell_h;
            let inset = cal.cell_w * 0.3;
            // For col 9: don't sample rightward (attack meter is there).
            // For col 0: don't sample leftward (potential left border).
            let left_inset = if col == 0 { 0.0 } else { inset };
            let right_inset = if col == 9 { 0.0 } else { inset };

            let mut solid_count = 0;
            for &dy in &[-inset, 0.0, inset] {
                for &dx in &[-left_inset, 0.0, right_inset] {
                    let px = (cx + dx).max(0.0) as u32;
                    let py = (cy + dy).max(0.0) as u32;
                    let (r, g, b) = sample_pixel(pixels, width, px, py);
                    let bright = r as u32 + g as u32 + b as u32;
                    if bright > FILL_THRESHOLD {
                        // Require high saturation to reject borders (sat ~0.25-0.30)
                        // and ghost pieces. Real locked pieces have sat > 0.40.
                        let (_, s, _) = rgb_to_hsl(r, g, b);
                        if s > 0.35 {
                            solid_count += 1;
                        }
                    }
                }
            }
            raw_board[row][col] = solid_count >= 5;
        }
    }

    // Column 9: the TETR.IO attack meter (orange bar) overlaps this column.
    // Clear cells that are orange (L-piece hue ~15-40°). Real non-L pieces are kept.
    // Also count how many orange cells — if 5+, it's definitely the meter.
    let mut col9_orange = 0usize;
    for r in 0..20 {
        if raw_board[r][9] {
            let cx = cal.board_x as f64 + 9.5 * cal.cell_w;
            let cy = cal.board_y as f64 + (r as f64 + 0.5) * cal.cell_h;
            let (pr, pg, pb) = sample_pixel(pixels, width, cx as u32, cy as u32);
            let (h, s, _) = rgb_to_hsl(pr, pg, pb);
            // Orange meter: hue 15-45°, high saturation
            if s > 0.30 && h > 10.0 && h < 50.0 {
                col9_orange += 1;
            }
        }
    }
    // If 5+ orange cells in col 9, it's the meter — clear all orange cells
    if col9_orange >= 5 {
        for r in 0..20 {
            if raw_board[r][9] {
                let cx = cal.board_x as f64 + 9.5 * cal.cell_w;
                let cy = cal.board_y as f64 + (r as f64 + 0.5) * cal.cell_h;
                let (pr, pg, pb) = sample_pixel(pixels, width, cx as u32, cy as u32);
                let (h, s, _) = rgb_to_hsl(pr, pg, pb);
                if s > 0.30 && h > 10.0 && h < 50.0 {
                    raw_board[r][9] = false;
                }
            }
        }
    }

    let current_piece = detect_piece_at_spawn(pixels, width, cal);
    let queue = detect_queue_scan(pixels, width, cal);
    let hold = detect_hold_scan(pixels, width, cal);

    let board = extract_stack(&raw_board);

    VisionState { board, current_piece, hold, queue }
}

/// Flood-fill from the bottom to find only the locked stack.
fn extract_stack(raw: &[[bool; 10]; 20]) -> [[bool; 10]; 20] {
    let mut stack = [[false; 10]; 20];
    let mut visited = [[false; 10]; 20];
    let mut queue: Vec<(usize, usize)> = Vec::with_capacity(200);

    for col in 0..10 {
        if raw[19][col] {
            queue.push((19, col));
            visited[19][col] = true;
            stack[19][col] = true;
        }
    }

    let mut head = 0;
    while head < queue.len() {
        let (r, c) = queue[head];
        head += 1;
        for (dr, dc) in [(-1i32, 0), (1, 0), (0, -1), (0, 1)] {
            let nr = r as i32 + dr;
            let nc = c as i32 + dc;
            if nr < 0 || nr >= 20 || nc < 0 || nc >= 10 { continue; }
            let (nr, nc) = (nr as usize, nc as usize);
            if !visited[nr][nc] && raw[nr][nc] {
                visited[nr][nc] = true;
                stack[nr][nc] = true;
                queue.push((nr, nc));
            }
        }
    }
    stack
}

pub fn debug_state(vis: &VisionState) {
    eprintln!("[Vision] Current: {:?}  Hold: {:?}  Queue: {:?}",
        vis.current_piece, vis.hold, vis.queue);
    let filled: usize = vis.board.iter().flat_map(|r| r.iter()).filter(|&&c| c).count();
    eprintln!("[Vision] Filled cells: {}", filled);
    for row in 0..20 {
        let mut line = String::from("|");
        for col in 0..10 {
            line.push(if vis.board[row][col] { 'X' } else { '.' });
        }
        line.push('|');
        eprintln!("  {}", line);
    }
}

pub fn debug_colors(pixels: &[u8], width: u32, cal: &Calibration) {
    eprintln!("[Color] Spawn area (rows 0-2, cols 3-6):");
    for row in 0..3 {
        for col in 3..7 {
            let px = cal.board_x as f64 + (col as f64 + 0.5) * cal.cell_w;
            let py = cal.board_y as f64 + (row as f64 + 0.5) * cal.cell_h;
            let (r, g, b) = sample_pixel(pixels, width, px as u32, py as u32);
            let bright = r as u32 + g as u32 + b as u32;
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let piece = identify_piece(r, g, b);
            eprintln!("  [{},{}] rgb=({:3},{:3},{:3}) bright={:3} hsl=({:.0},{:.2},{:.2}) → {:?}",
                row, col, r, g, b, bright, h, s, l, piece);
        }
    }
}

fn sample_pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> (u8, u8, u8) {
    let idx = ((y * width + x) * 4) as usize;
    if idx + 2 >= pixels.len() { return (0, 0, 0); }
    (pixels[idx], pixels[idx + 1], pixels[idx + 2])
}

fn identify_piece(r: u8, g: u8, b: u8) -> Option<PieceType> {
    let brightness = r as u32 + g as u32 + b as u32;
    if brightness < 150 { return None; } // empty cells are 0-78, pieces are 150+

    let (h, s, _l) = rgb_to_hsl(r, g, b);
    if s < 0.40 { return None; } // rejects desaturated background (sat ~0.15-0.28)

    const HUES: &[(PieceType, f64, f64)] = &[
        (PieceType::Z, 0.0, 20.0),     // red:        340-360, 0-20
        (PieceType::L, 28.0, 15.0),    // orange:     13-43
        (PieceType::O, 52.0, 12.0),    // yellow:     40-64
        (PieceType::S, 90.0, 30.0),    // lime/green: 60-120
        (PieceType::I, 160.0, 30.0),   // cyan/teal:  130-190
        (PieceType::J, 235.0, 20.0),   // blue:       215-255
        (PieceType::T, 290.0, 30.0),   // purple:     260-320
    ];

    let mut best: Option<(PieceType, f64)> = None;
    for &(piece, hue, range) in HUES {
        let mut dist = (h - hue).abs();
        if dist > 180.0 { dist = 360.0 - dist; }
        if dist <= range {
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((piece, dist));
            }
        }
    }
    best.map(|(p, _)| p)
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let r = r as f64 / 255.0;
    let g = g as f64 / 255.0;
    let b = b as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 { return (0.0, 0.0, l); }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if (max - r).abs() < 1e-6 {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) * 60.0
    } else if (max - g).abs() < 1e-6 {
        ((b - r) / d + 2.0) * 60.0
    } else {
        ((r - g) / d + 4.0) * 60.0
    };
    (h, s, l)
}

fn detect_piece_at_spawn(pixels: &[u8], width: u32, cal: &Calibration) -> Option<PieceType> {
    let mut votes = [0u32; 7];
    for row in 0..3 {
        for col in 3..7 {
            let px = cal.board_x as f64 + (col as f64 + 0.5) * cal.cell_w;
            let py = cal.board_y as f64 + (row as f64 + 0.5) * cal.cell_h;
            let (r, g, b) = sample_pixel(pixels, width, px as u32, py as u32);
            if let Some(piece) = identify_piece(r, g, b) {
                votes[piece as usize] += 1;
            }
        }
    }
    let (best_idx, &best_count) = votes.iter().enumerate().max_by_key(|&(_, &v)| v)?;
    if best_count >= 2 {
        Some(PieceType::ALL[best_idx])
    } else {
        None
    }
}

/// Scan the right side of the capture to find queue pieces.
/// Instead of fixed positions, finds colored pixel clusters.
fn detect_queue_scan(pixels: &[u8], width: u32, cal: &Calibration) -> Vec<PieceType> {
    let board_right = cal.board_x as f64 + cal.cell_w * 10.0;
    // Start scanning 1.5 cells right of board (skip attack meter/border)
    // End 4.5 cells right (don't scan too far into border/decoration)
    let scan_x_min = board_right + cal.cell_w * 1.5;
    let scan_x_max = (width as f64).min(board_right + cal.cell_w * 4.5);
    // Scan from board top to ~12 cells down
    let scan_y_min = cal.board_y as f64 - cal.cell_h;
    let scan_y_max = cal.board_y as f64 + cal.cell_h * 12.0;

    let step = cal.cell_w * 0.3;

    // Find all colored pixels in the queue area
    let mut colored_points: Vec<(f64, PieceType)> = Vec::new();
    let mut y = scan_y_min.max(0.0);
    while y <= scan_y_max {
        let mut x = scan_x_min;
        while x <= scan_x_max {
            let (r, g, b) = sample_pixel(pixels, width, x as u32, y as u32);
            if let Some(piece) = identify_piece(r, g, b) {
                colored_points.push((y, piece));
            }
            x += step;
        }
        y += step;
    }

    if colored_points.is_empty() { return Vec::new(); }

    // Sort by y
    colored_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    // Cluster by gaps between consecutive colored points.
    // Within a piece, consecutive colored samples are ~15px apart (scan step).
    // Between pieces, the gap is ~25-35px (no colored pixels in the gap).
    let mut clusters: Vec<Vec<PieceType>> = Vec::new();
    let mut current_cluster: Vec<PieceType> = vec![colored_points[0].1];
    let mut prev_y = colored_points[0].0;

    for &(y, piece) in &colored_points[1..] {
        if y - prev_y > cal.cell_h * 0.5 {
            // Gap larger than half a cell — new piece
            clusters.push(std::mem::take(&mut current_cluster));
        }
        current_cluster.push(piece);
        prev_y = y;
    }
    clusters.push(current_cluster);

    // For each cluster, take the majority piece type
    let mut queue = Vec::new();
    for cluster in &clusters {
        let mut votes = [0u32; 7];
        for &piece in cluster {
            votes[piece as usize] += 1;
        }
        let (best_idx, &best_count) = votes.iter().enumerate().max_by_key(|&(_, &v)| v).unwrap();
        // Require at least 5 colored pixels — a real piece has 10+, noise has 1-3
        if best_count >= 5 {
            queue.push(PieceType::ALL[best_idx]);
        }
    }

    queue.truncate(5);
    queue
}

/// Scan the left side of the capture to find the held piece.
fn detect_hold_scan(pixels: &[u8], width: u32, cal: &Calibration) -> Option<PieceType> {
    // Scan from capture left edge to 1 cell left of the board
    let scan_x_min = 0.0f64;
    let scan_x_max = (cal.board_x as f64 - cal.cell_w).max(0.0);
    // Scan from board top area to about 4 cells down
    let scan_y_min = (cal.board_y as f64 - cal.cell_h).max(0.0);
    let scan_y_max = cal.board_y as f64 + cal.cell_h * 4.0;

    let step = cal.cell_w * 0.3;

    let mut votes = [0u32; 7];
    let mut y = scan_y_min;
    while y <= scan_y_max {
        let mut x = scan_x_min;
        while x <= scan_x_max {
            let (r, g, b) = sample_pixel(pixels, width, x as u32, y as u32);
            if let Some(piece) = identify_piece(r, g, b) {
                votes[piece as usize] += 1;
            }
            x += step;
        }
        y += step;
    }

    let (best_idx, &best_count) = votes.iter().enumerate().max_by_key(|&(_, &v)| v)?;
    if best_count >= 5 {
        Some(PieceType::ALL[best_idx])
    } else {
        None
    }
}

// ─── Platform-specific region selection ─────────────────────────────

/// Returns (x, y, width, height) of the user's selection.
#[cfg(target_os = "linux")]
fn platform_select_region() -> Option<(u32, u32, u32, u32)> {
    let output = std::process::Command::new("slurp")
        .output()
        .ok()?;
    if !output.status.success() { return None; }
    let region = String::from_utf8(output.stdout).ok()?.trim().to_string();
    let parts: Vec<&str> = region.split_whitespace().collect();
    if parts.len() != 2 { return None; }
    let pos: Vec<u32> = parts[0].split(',').filter_map(|s| s.parse().ok()).collect();
    let size: Vec<u32> = parts[1].split('x').filter_map(|s| s.parse().ok()).collect();
    if pos.len() != 2 || size.len() != 2 { return None; }
    Some((pos[0], pos[1], size[0], size[1]))
}

/// macOS: Use screencapture -i to let user drag-select, then read the image dimensions.
#[cfg(target_os = "macos")]
fn platform_select_region() -> Option<(u32, u32, u32, u32)> {
    eprintln!("[Cal] Drag to select the board area...");
    // screencapture -i captures a selection and saves to file
    // -J selection outputs JSON with selection rect on newer macOS
    // Fallback: capture to temp file, read dimensions, ask user for position
    let tmp = "/tmp/tetrio-bot-calibrate.png";
    let status = std::process::Command::new("screencapture")
        .args(["-i", "-x", tmp])
        .status()
        .ok()?;
    if !status.success() { return None; }

    // Get image dimensions via sips
    let output = std::process::Command::new("sips")
        .args(["-g", "pixelWidth", "-g", "pixelHeight", tmp])
        .output()
        .ok()?;
    let sips_out = String::from_utf8_lossy(&output.stdout);
    let mut w = 0u32;
    let mut h = 0u32;
    for line in sips_out.lines() {
        let line = line.trim();
        if line.starts_with("pixelWidth:") {
            w = line.split(':').nth(1)?.trim().parse().ok()?;
        } else if line.starts_with("pixelHeight:") {
            h = line.split(':').nth(1)?.trim().parse().ok()?;
        }
    }
    if w == 0 || h == 0 { return None; }

    // screencapture -i doesn't tell us the position, so we need to find it.
    // Use the mouse position at capture start as an approximation.
    // For a more robust approach, we capture the full screen and diff.
    // Simple approach: ask the system for the mouse location via cliclick or python
    let pos_output = std::process::Command::new("python3")
        .args(["-c", "from Quartz import NSEvent; p = NSEvent.mouseLocation(); from AppKit import NSScreen; h = NSScreen.mainScreen().frame().size.height; print(f'{int(p.x)},{int(h - p.y)}')"])
        .output()
        .ok()?;
    let pos_str = String::from_utf8_lossy(&pos_output.stdout).trim().to_string();
    let coords: Vec<u32> = pos_str.split(',').filter_map(|s| s.parse().ok()).collect();

    // The selection's top-left is approximately at (mouse_x - w, mouse_y - h)
    // since the user drags from top-left to bottom-right and releases at bottom-right.
    // This is imprecise; the user may need to re-select if off.
    let (mx, my) = if coords.len() == 2 { (coords[0], coords[1]) } else { (0, 0) };
    let sel_x = mx.saturating_sub(w);
    let sel_y = my.saturating_sub(h);

    eprintln!("[Cal] Detected selection: {}x{} at ({},{})", w, h, sel_x, sel_y);
    eprintln!("[Cal] If position looks wrong, try selecting from top-left to bottom-right.");
    Some((sel_x, sel_y, w, h))
}
