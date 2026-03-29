# tetriov2 (macOS)

A TETR.IO bot that plays the game via screen capture and virtual keyboard input. Uses a beam search AI engine with SRS piece definitions, BFS move generation, and aggressive Tetris-stacking evaluation.

> This is the **macOS branch**. For Linux (Wayland/Hyprland), see the `master` branch.

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
    screen.rs - Screenshot capture (screencapture + sips)
    input.rs  - Virtual keyboard (CGEvent / Core Graphics)
```

## Requirements

- **macOS 12+** (Monterey or later)
- **Rust toolchain** (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- **Accessibility permissions** for your terminal app

### Granting Accessibility access

The bot uses Core Graphics events to send keystrokes. macOS requires explicit permission:

1. Open **System Settings > Privacy & Security > Accessibility**
2. Click the **+** button and add your terminal app (Terminal.app, iTerm2, Alacritty, etc.)
3. If using VS Code terminal, add Visual Studio Code instead

Without this, key events won't reach the browser.

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

# Run (will prompt you to drag-select the board area)
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
2. Run the bot - a crosshair will appear for you to drag-select the 10x20 board grid
3. Switch to the browser within 3 seconds
4. The bot plays automatically. Ctrl+C to stop.

The bot checks if a browser window is focused before sending keys. If you switch away, it pauses.

## Platform differences from Linux

| Component | Linux | macOS |
|-----------|-------|-------|
| Screenshot | `grim -g` (Wayland) | `screencapture -R` + `sips` |
| Region select | `slurp` | `screencapture -i` |
| Keyboard input | `/dev/uinput` (kernel) | `CGEvent` (Core Graphics) |
| Focus check | `hyprctl activewindow` | `osascript` (AppleScript) |
| Dependencies | `libc` | `core-graphics`, `core-foundation` |

## Benchmark

```
Pieces: 500 | Lines: 198 | Tetrises: 44 | T-spins: 1
Attack: 239 (0.48/piece)
```

## How it works

1. **Capture** - Takes a screenshot of the game region via `screencapture`
2. **Detect** - Identifies the current piece at spawn by color (HSL hue matching), scans for queue pieces and held piece
3. **Board** - Tracks board state internally from AI placements (not vision) for reliability
4. **Search** - Beam search expands all possible placements for current piece + queue, evaluates each resulting board
5. **Input** - Sends hold/rotation/movement/hard-drop keys via CGEvent
6. **Repeat** - Waits for next piece spawn, captures again

## Troubleshooting

- **Keys not reaching the game**: Check Accessibility permissions. The terminal app must be listed.
- **Screenshot fails**: `screencapture` is built into macOS. If it fails, check if Screen Recording permissions are needed (System Settings > Privacy > Screen Recording).
- **Board position wrong**: Try selecting just the 10x20 grid more precisely. The selection goes from top-left to bottom-right.
- **Retina display**: The bot should handle Retina scaling automatically since `screencapture` captures at screen resolution.
