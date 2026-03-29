mod screen;
mod vision;
mod input;

use std::io::Write;
use std::time::{Duration, Instant};
use std::thread;
use std::fs;
use std::process::{Command, Child};

use tetrio_engine::{
    Board, GameState, PieceType, SearchConfig, Weights, find_best_move,
};

fn log(msg: &str) {
    eprintln!("{}", msg);
    std::io::stderr().flush().ok();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--benchmark" || a == "-b") {
        run_benchmark();
        return;
    }

    let debug = args.iter().any(|a| a == "--debug" || a == "-d");

    let beam_width: usize = args
        .iter()
        .position(|a| a == "--beam" || a == "-w")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    let config = SearchConfig {
        beam_width,
        weights: Weights::default_aggressive(),
    };

    log("=== TETR.IO Bot — S+ Beam Search Engine ===");
    log(&format!("Beam width: {}  Debug: {}", beam_width, debug));
    log("");
    log("1. Open TETR.IO in your browser and start a game");
    log("2. slurp will appear — drag to select the 10x20 BOARD GRID");
    log("   (just the playing field, not hold/queue)");
    log("");

    let cal = match vision::manual_calibrate() {
        Some(c) => c,
        None => {
            log("[!] Calibration failed. Install slurp: pacman -S slurp");
            return;
        }
    };

    // Debug: create output directory and start screen recording
    let debug_dir = "/tmp/bot-debug";
    let mut recorder: Option<Child> = None;
    if debug {
        let _ = fs::remove_dir_all(debug_dir);
        fs::create_dir_all(debug_dir).ok();
        log(&format!("[Debug] Saving screenshots to {}/", debug_dir));

        // Start screen recording of the game region
        let geom = format!("{},{} {}x{}", cal.capture_x, cal.capture_y, cal.capture_w, cal.capture_h);
        let rec_path = format!("{}/game.mp4", debug_dir);
        match Command::new("wf-recorder")
            .args(["-g", &geom, "-f", &rec_path, "--no-damage"])
            .spawn()
        {
            Ok(child) => {
                recorder = Some(child);
                log(&format!("[Debug] Recording screen to {}", rec_path));
            }
            Err(e) => {
                log(&format!("[Debug] wf-recorder not available ({}), skipping recording", e));
            }
        }
    }

    log("[*] Creating virtual keyboard via /dev/uinput...");
    let mut keyboard = match input::VirtualKeyboard::new() {
        Ok(kb) => kb,
        Err(e) => {
            log(&format!("[!] {}", e));
            log("[!] Make sure you have write access to /dev/uinput");
            return;
        }
    };
    log("[*] Bot active! Switch to your browser window NOW.");
    log("[*] Starting in 3 seconds...");
    thread::sleep(Duration::from_secs(3));
    log("[*] Running! Keep TETR.IO focused.");

    let mut game_state = GameState::new();
    let mut last_hash = 0u64;
    let mut pieces = 0u64;
    let mut frame_count = 0u64;
    let mut no_piece_count = 0u64;
    let start_time = Instant::now();
    let mut internal_hold: Option<PieceType> = None; // track hold state ourselves

    // Debug log file
    let mut debug_log: Option<fs::File> = if debug {
        fs::File::create(format!("{}/moves.log", debug_dir)).ok()
    } else {
        None
    };

    loop {
        let frame_start = Instant::now();
        frame_count += 1;

        // 1. Capture just the game region
        let capture_start = Instant::now();
        let Some((pixels, w, h)) = screen::capture_region(
            cal.capture_x, cal.capture_y, cal.capture_w, cal.capture_h
        ) else {
            log("[!] grim capture failed, retrying...");
            thread::sleep(Duration::from_millis(500));
            continue;
        };
        let capture_ms = capture_start.elapsed().as_millis();

        if frame_count <= 3 {
            log(&format!("[Frame {}] Screenshot: {}x{} ({} bytes, {}ms)",
                frame_count, w, h, pixels.len(), capture_ms));
        }

        // 2. Extract game state from pixels
        let vis = vision::extract_state(&pixels, w, &cal);

        // Debug: always show first 5 frames + save screenshots for first 10
        if frame_count <= 5 {
            vision::debug_state(&vis);
            vision::debug_colors(&pixels, w, &cal);
            // Always save first 10 screenshots so we can diagnose
            let dir = "/tmp/bot-debug";
            let _ = fs::create_dir_all(dir);
            save_ppm(&pixels, w, h, &format!("{}/frame_{:04}.ppm", dir, frame_count));
            log(&format!("[Debug] Saved {}/frame_{:04}.ppm", dir, frame_count));
        }

        // 3. Only act on state changes
        let hash = state_hash(&vis);
        if hash == last_hash {
            thread::sleep(Duration::from_millis(30));
            continue;
        }
        last_hash = hash;

        // 4. Need a visible current piece to act
        let filled: usize = vis.board.iter().flat_map(|r| r.iter()).filter(|&&c| c).count();
        if vis.current_piece.is_none() {
            no_piece_count += 1;
            if no_piece_count <= 5 || no_piece_count % 50 == 0 {
                log(&format!("[Vision] No piece detected (filled={}, frame={}). Waiting...",
                    filled, frame_count));
            }
            thread::sleep(Duration::from_millis(30));
            continue;
        }
        no_piece_count = 0;

        let current = vis.current_piece.unwrap();

        // 5. Board state management — use INTERNAL tracking, not vision.
        // Vision is unreliable (attack meter, wallpaper transparency, ghost pieces).
        // Only use vision board when game resets (filled drops to near 0).
        let vision_filled = filled;
        let internal_filled = {
            let mut c = 0usize;
            for row in 0..20 {
                for col in 0..10 {
                    if game_state.board.rows[row] & (1 << col) != 0 { c += 1; }
                }
            }
            c
        };

        // Detect game reset: vision shows near-empty but internal has many cells
        if vision_filled < 6 && internal_filled > 20 {
            log(&format!("[Reset] Game restarted (internal={}, vision={}). Resetting board.",
                internal_filled, vision_filled));
            game_state.board = Board::new();
            internal_hold = None;
            game_state.hold = None;
            game_state.b2b = false;
            game_state.combo = 0;
        }
        // On first piece (empty internal board), seed from vision
        if internal_filled == 0 && vision_filled > 0 {
            let mut board = Board::new();
            for vis_row in 0..20 {
                let engine_row = 19 - vis_row;
                for col in 0..10 {
                    if vis.board[vis_row][col] {
                        board.set(col as i8, engine_row as i8);
                    }
                }
            }
            board.rebuild_col_heights();
            game_state.board = board;
        }
        // Otherwise: keep using internal board (updated after each placement)

        // Use vision hold if detected, otherwise use our internal tracking
        game_state.hold = vis.hold.or(internal_hold);

        // 6. Run beam search
        let t0 = Instant::now();
        let result = find_best_move(current, &vis.queue, &game_state, &config);
        let search_ms = t0.elapsed().as_secs_f64() * 1000.0;

        if let Some(mv) = result {
            pieces += 1;
            let elapsed = start_time.elapsed().as_secs_f64();
            let pps = if elapsed > 0.0 { pieces as f64 / elapsed } else { 0.0 };

            let play_piece = if mv.use_hold {
                let held = game_state.hold;
                // Update internal hold tracking
                internal_hold = Some(current);
                held.unwrap_or_else(|| vis.queue.first().copied().unwrap_or(current))
            } else {
                current
            };

            let rotation = mv.placement.rot as u8;
            let spawn_x = play_piece.spawn_x();
            let dx = mv.placement.x - spawn_x;

            // Always log for debug, otherwise log first 50 + every 25th
            if debug || pieces <= 50 || pieces % 25 == 0 {
                log(&format!(
                    "[Bot] #{:<4} cur={:?} play={:?} col={} rot={} hold={:<5} dx={:+} | {:.0}ms pps={:.1} | filled={} q={:?}",
                    pieces, current, play_piece, mv.placement.x, rotation,
                    mv.use_hold, dx, search_ms, pps, filled, vis.queue
                ));
            }

            // Debug: save screenshot + board state + move details
            if debug && pieces <= 100 {
                // Save screenshot as PPM
                save_ppm(&pixels, w, h, &format!("{}/move_{:04}.ppm", debug_dir, pieces));

                // Write detailed move info to log
                if let Some(ref mut f) = debug_log {
                    let _ = writeln!(f, "=== Move {} (frame {}) ===", pieces, frame_count);
                    let _ = writeln!(f, "Current: {:?}  Hold: {:?}  Queue: {:?}", current, vis.hold, vis.queue);
                    let _ = writeln!(f, "Decision: play={:?} x={} rot={} hold={} dx={:+} spawn_x={}",
                        play_piece, mv.placement.x, rotation, mv.use_hold, dx, spawn_x);
                    let _ = writeln!(f, "Score: {:.2}  Search: {:.0}ms  Filled: {}", mv.score, search_ms, filled);
                    let _ = writeln!(f, "Keys: {}{}{}SPACE",
                        if mv.use_hold { "C " } else { "" },
                        match rotation { 1 => "UP ", 2 => "UP UP ", 3 => "Z ", _ => "" },
                        if dx > 0 { format!("RIGHT×{} ", dx) }
                        else if dx < 0 { format!("LEFT×{} ", dx.unsigned_abs()) }
                        else { String::new() }
                    );
                    // Board grid
                    let _ = writeln!(f, "Board (vision, after stack extraction):");
                    for row in 0..20 {
                        let mut line = String::from("|");
                        for col in 0..10 {
                            line.push(if vis.board[row][col] { 'X' } else { '.' });
                        }
                        line.push('|');
                        let _ = writeln!(f, "  {}", line);
                    }
                    let _ = writeln!(f, "Engine col_heights: {:?}", game_state.board.col_heights());
                    let _ = writeln!(f, "");
                    let _ = f.flush();
                }
            }

            // 7. Send keystrokes via virtual keyboard (only if game focused)
            if !keyboard.execute_move(mv.use_hold, rotation, dx) {
                if pieces <= 5 || pieces % 50 == 0 {
                    log("[!] Game not focused — keys NOT sent. Click on TETR.IO!");
                }
                thread::sleep(Duration::from_millis(500));
                continue;
            }

            // 8. Update internal board state (don't rely on vision for board)
            game_state.board.place_piece(mv.placement.piece, mv.placement.x, mv.placement.y, mv.placement.rot);
            let lines = game_state.board.clear_lines();
            if lines > 0 {
                game_state.combo += 1;
                if pieces <= 50 || pieces % 25 == 0 {
                    log(&format!("  → cleared {} lines! combo={}", lines, game_state.combo));
                }
            } else {
                game_state.combo = 0;
            }

            // Wait for TETR.IO to process hard drop + spawn next piece
            thread::sleep(Duration::from_millis(70));
        } else {
            log("[Bot] No valid move found (game over?)");
            thread::sleep(Duration::from_millis(100));
        }

        // Frame pacing
        let frame_time = frame_start.elapsed();
        if frame_time < Duration::from_millis(25) {
            thread::sleep(Duration::from_millis(25) - frame_time);
        }
    }

    // Cleanup recorder (unreachable due to loop, but for completeness)
    #[allow(unreachable_code)]
    if let Some(mut rec) = recorder {
        let _ = rec.kill();
    }
}

fn save_ppm(pixels: &[u8], width: u32, height: u32, path: &str) {
    let mut data = format!("P6\n{} {}\n255\n", width, height).into_bytes();
    // pixels is RGBA, PPM needs RGB
    for chunk in pixels.chunks(4) {
        if chunk.len() >= 3 {
            data.push(chunk[0]);
            data.push(chunk[1]);
            data.push(chunk[2]);
        }
    }
    let _ = fs::write(path, &data);
}

fn state_hash(vis: &vision::VisionState) -> u64 {
    let mut hash = 0u64;
    for (i, row) in vis.board.iter().enumerate() {
        for (j, &cell) in row.iter().enumerate() {
            if cell { hash ^= 1u64 << ((i * 10 + j) % 64); }
        }
    }
    if let Some(p) = vis.current_piece {
        hash ^= (p as u64 + 1) << 56;
    }
    hash
}

fn run_benchmark() {
    use tetrio_engine::{PieceType, attack};

    println!("=== TETR.IO Bot Benchmark ===\n");

    let config = SearchConfig {
        beam_width: 400,
        weights: Weights::default_aggressive(),
    };

    {
        let board = Board::new();
        let start = Instant::now();
        let iterations = 10_000;
        for _ in 0..iterations {
            for piece in PieceType::ALL {
                let _ = tetrio_engine::movegen::generate_placements_with_drops(&board, piece);
            }
        }
        let per_piece = start.elapsed() / (iterations * 7);
        println!("Move generation: {:?} per piece", per_piece);
    }

    {
        let state = GameState::new();
        let queue = [PieceType::I, PieceType::T, PieceType::S, PieceType::Z, PieceType::J];
        for &width in &[50, 100, 200, 400] {
            let cfg = SearchConfig { beam_width: width, weights: config.weights.clone() };
            let start = Instant::now();
            for _ in 0..100 {
                let _ = find_best_move(PieceType::T, &queue, &state, &cfg);
            }
            let per_search = start.elapsed() / 100;
            println!("Search (beam={}): {:?} per move", width, per_search);
        }
    }

    {
        println!("\nSimulating 500 pieces...");
        let start = Instant::now();
        let mut state = GameState::new();
        let mut rng_state = 42u64;

        let gen_bag = |rng: &mut u64| -> Vec<PieceType> {
            let mut bag = PieceType::ALL.to_vec();
            for i in (1..bag.len()).rev() {
                *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                let j = (*rng >> 33) as usize % (i + 1);
                bag.swap(i, j);
            }
            bag
        };

        let mut piece_stream: Vec<PieceType> = Vec::new();
        let mut stream_idx = 0usize;
        for _ in 0..100 {
            piece_stream.extend(gen_bag(&mut rng_state));
        }

        let (mut total_lines, mut total_attack, mut pieces, mut tetrises, mut tspins) =
            (0u32, 0u32, 0u32, 0u32, 0u32);

        for _ in 0..500 {
            let current = piece_stream[stream_idx];
            stream_idx += 1;
            let queue: Vec<PieceType> = piece_stream[stream_idx..stream_idx + 5].to_vec();

            if let Some(mv) = find_best_move(current, &queue, &state, &config) {
                let play_piece = if mv.use_hold {
                    if let Some(held) = state.hold {
                        state.hold = Some(current);
                        held
                    } else {
                        state.hold = Some(current);
                        stream_idx += 1;
                        queue[0]
                    }
                } else {
                    current
                };

                if !state.board.collides(play_piece, mv.placement.x, mv.placement.y, mv.placement.rot) {
                    let pr = state.apply_placement(&mv.placement);
                    total_lines += pr.lines_cleared;
                    total_attack += pr.attack;
                    if pr.lines_cleared == 4 { tetrises += 1; }
                    if matches!(pr.clear_type, attack::ClearType::TSpinDouble | attack::ClearType::TSpinTriple | attack::ClearType::TSpinSingle) {
                        tspins += 1;
                    }
                }
                pieces += 1;
            } else {
                println!("Game over at piece {}", pieces);
                break;
            }
            if state.board.max_height() >= 20 {
                println!("Topped out at piece {}", pieces);
                break;
            }
        }

        let elapsed = start.elapsed();
        println!("Pieces: {} | Lines: {} | Tetrises: {} | T-spins: {}", pieces, total_lines, tetrises, tspins);
        println!("Attack: {} ({:.2}/piece)", total_attack, total_attack as f64 / pieces.max(1) as f64);
        println!("Time: {:?} ({:?}/piece)", elapsed, elapsed / pieces.max(1));
        println!("{:?}", state.board);
    }
}
