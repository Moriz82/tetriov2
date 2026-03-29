# tetriov2

A TETR.IO bot that plays the game via screen capture and virtual keyboard input. Uses a beam search AI engine with SRS piece definitions, BFS move generation, and aggressive Tetris-stacking evaluation.

## Architecture

```
tetriov2/
  engine/     - AI engine (pure Rust, no platform deps)
    piece.rs  - SRS piece cells, rotation, kick tables
    board.rs  - Bitboard (u16 per row), collision, features
    movegen.rs- BFS placement finder + fast column-drop
    eval.rs   - Heuristic evaluation (holes, bumpiness, wells, clears)
    search.rs - Beam search over piece queue
    attack.rs - Line clear classification, attack calculation
    game.rs   - Game state (board, hold, b2b, combo)
  bot/        - Binary that plays TETR.IO
    main.rs   - Game loop: capture -> detect -> search -> input
    vision.rs - Color-based piece/board detection from screenshots
    screen.rs - Screenshot capture (grim on Linux, screencapture on macOS)
    input.rs  - Virtual keyboard (/dev/uinput on Linux, CGEvent on macOS)
```

## Requirements

### Linux (Wayland/Hyprland)

- `grim` - screenshot tool
- `slurp` - region selection tool
- `/dev/uinput` write access (for virtual keyboard)
- Rust toolchain

```bash
# Arch
pacman -S grim slurp

# uinput access (add yourself to input group or use sudo)
sudo chmod 666 /dev/uinput
```

### macOS

See the `macos` branch. Requires:
- Accessibility permissions for your terminal (System Settings > Privacy > Accessibility)
- Rust toolchain

## TETR.IO Settings

Set these in TETR.IO Config > Handling before running:

| Setting | Value |
|---------|-------|
| DAS | 0-1F |
| ARR | 0F |
| SDF | Infinity |
| Prevent Accidental Hard Drops | OFF |
| IRS (Rotation Buffering) | OFF |
| IHS (Hold Buffering) | OFF |

## Usage

```bash
# Build
cargo build --release

# Run (will prompt you to select the board with slurp)
cargo run --release

# Custom beam width (lower = faster, higher = smarter)
cargo run --release -- --beam 50    # fast (~4 PPS)
cargo run --release -- --beam 200   # smart (~2 PPS)

# Debug mode (saves screenshots to /tmp/bot-debug/)
cargo run --release -- --debug

# Benchmark (no game needed, tests AI in simulation)
cargo run --release -- --benchmark
```

### How to play

1. Open TETR.IO in your browser and start a game (40L, Blitz, or custom)
2. Run the bot - `slurp` will appear for you to drag-select the 10x20 board grid
3. Switch to the browser within 3 seconds
4. The bot plays automatically. Ctrl+C to stop.

The bot only sends keys when the TETR.IO window is focused. If you switch away, it pauses.

## Benchmark

```
Pieces: 500 | Lines: 198 | Tetrises: 44 | T-spins: 1
Attack: 239 (0.48/piece)
```

## How it works

1. **Capture** - Takes a screenshot of the game region via `grim`
2. **Detect** - Identifies the current piece at spawn by color (HSL hue matching), scans for queue pieces and held piece
3. **Board** - Tracks board state internally from AI placements (not vision) for reliability
4. **Search** - Beam search expands all possible placements for current piece + queue, evaluates each resulting board
5. **Input** - Sends hold/rotation/movement/hard-drop keys via virtual keyboard
6. **Repeat** - Waits for next piece spawn, captures again
