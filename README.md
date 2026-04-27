# Terminal Chess in Rust

CIS 1905 final project; Madhav Sharma, Spring 2026.

This is a fully-playable two-player chess game that runs in your terminal.
The board is rendered with [`ratatui`](https://ratatui.rs/) and the whole
event loop is async on top of [`tokio`](https://tokio.rs/). I tried to use
the project as an excuse to actually exercise every topic from the
syllabus.

```
┌ Chess ───────────────────────────────┐┌ Status ──────────────────────────────┐
│   a  b  c  d  e  f  g  h             ││White to move                         │
│ 8  ♜  ♞  ♝  ♛  ♚  ♝  ♞  ♜  8         ││Game in progress                      │
│ 7  ♟  ♟  ♟  ♟  ♟  ♟  ♟  ♟  7         ││                                      │
│ 6                          6         │└──────────────────────────────────────┘
│ 5                          5         │┌ Moves ───────────────────────────────┐
│ 4                          4         ││                                      │
│ 3                          3         ││                                      │
│ 2  ♙  ♙  ♙  ♙  ♙  ♙  ♙  ♙  2         ││                                      │
│ 1  ♖  ♘  ♗  ♕  ♔  ♗  ♘  ♖  1         ││                                      │
│   a  b  c  d  e  f  g  h             ││                                      │
└──────────────────────────────────────┘└──────────────────────────────────────┘
```

> The above is the **actual** rendered output, captured via ratatui's
> `TestBackend`; not a hand-drawn approximation. You can regenerate
> it yourself with `cargo test --test snapshot -- --nocapture`. In a real
> terminal it's in colour, with green legal-move highlights, a yellow
> selected-piece highlight, and a blue cursor.

---

### Things that work

**Game logic (rules engine)**
- [x] Legal move generation for all six piece types
- [x] **Castling** &mdash; kingside *and* queenside, with all the real
      conditions: castling rights, empty squares between, king not
      currently in check, king doesn't pass through an attacked square
- [x] **En passant** &mdash; including the one-move expiry of the e.p. target
- [x] **Pawn promotion** &mdash; with all four choices (Q, R, B, N), prompted
      after the move
- [x] **Check** detection (status panel says "X is in check!")
- [x] **Checkmate** detection (game ends, "X wins" displayed)
- [x] **Stalemate** detection (distinct from checkmate)
- [x] **50-move rule** auto-draw
- [x] **Insufficient-material** auto-draw (KvK, KvK+N, KvK+B)

**TUI (rendering & input)**
- [x] Board drawn with Unicode pieces, light/dark squares, file & rank labels
- [x] Cursor highlight (blue), selected-piece highlight (yellow), legal-move
      highlights (green)
- [x] Move history shown as numbered pairs ("1. e2e4 e7e5") in a side panel
- [x] Status panel showing whose turn it is and the game state
- [x] Info panel with controls and contextual messages
- [x] Async event loop: arrow keys, Enter/Space to confirm, Backspace to
      cancel, `n` for new game, `q`/Esc/Ctrl-C to quit, `Q`/`R`/`B`/`N`
      to choose promotion piece

**Beyond the proposal**
- [x] **FEN parser/serializer** for the board placement &mdash; round-trips
      `rnbqkbnr/pppppppp/...` correctly. The TA suggested this in the
      proposal feedback as a possible extension.
- [x] **Perft benchmark** &mdash; the standard chess move-generator
      verification &mdash; matching every published value through depth 4.
- [x] **Snapshot test of the real UI** using ratatui's `TestBackend`.
- [x] Optional **rayon-parallel AI suggester** behind the `--features ai`
      flag (one-ply for now &mdash; this is the stretch goal from the
      proposal; the architecture is there but the move evaluator is
      deliberately trivial).

### Future Work

- **PGN export** (only FEN placement is implemented). I can serialize the
  board but not the move history in PGN format.
- **Threefold repetition** draw detection (would need a position hash
  table; I have insufficient-material and 50-move, but not this).
- **Mouse input** (the project enables mouse capture but the
  `translate_event` function only handles keyboard).
- **A real AI**. The `ai.rs` module shows the parallel-iterator scaffolding
  but only does one-ply material evaluation; a proper minimax with alpha-beta
  was beyond the three-week timeline.

---

## How to build and run it

You need Rust 1.75+ (the project uses the 2021 edition) and a terminal
with Unicode + 24-bit colour. Any modern terminal &mdash; Ghostty, iTerm2,
the macOS default, Alacritty, kitty &mdash; works.

```bash
cd chess
cargo run --release
```

If you want to play with the optional AI module compiled in:

```bash
cargo run --release --features ai
```

### Controls

| Key                  | Does what                                       |
|----------------------|--------------------------------------------------|
| Arrow keys           | Move the cursor on the board                    |
| Enter / Space        | Pick up a piece, then put it down on a target   |
| Backspace            | Cancel the current selection                    |
| `n`                  | New game                                        |
| `q` / Esc / Ctrl-C   | Quit                                            |
| `Q` / `R` / `B` / `N`| Choose promotion piece (only after a pawn move  |
|                      | reaches the back rank)                          |

---

## Verification

`perft` is the de-facto standard for chess move-generator verification.
You count every leaf node reachable in N plies from the starting
position and compare against published values. Every chess engine
ever written has had to pass this test.

```bash
cargo run --release --example perft
```

You should see this:

```
perft from the standard starting position:
┌───────┬───────────┬─────────┬───────────┬─────────────┐
│ depth │   nodes   │ expect. │  time(ms) │   knodes/s  │
├───────┼───────────┼─────────┼───────────┼─────────────┤
│   1   │        20 │      20 │      0.01 │      1495.3 │  OK
│   2   │       400 │     400 │      0.45 │       897.4 │  OK
│   3   │      8902 │    8902 │      9.43 │       943.8 │  OK
│   4   │    197281 │  197281 │    167.53 │      1177.6 │  OK
└───────┴───────────┴─────────┴───────────┴─────────────┘
```

Hitting all four numbers exactly is mathematically equivalent to
"every piece moves correctly, castling works, en passant works,
promotion works, and we never let our own king walk into check".
There's no other way to land on those node counts.

### Test suite

```bash
cargo test               # 13 tests, ~50 ms total
cargo test -- --ignored  # adds the depth-4 perft test (~200 ms in release)
```

What each test covers:

| Test                                            | What it pins down                          |
|-------------------------------------------------|--------------------------------------------|
| `starting_position_has_20_moves`                | basic move count from initial position     |
| `fools_mate_is_mate`                            | check + checkmate detection                |
| `stalemate_detected`                            | stalemate is distinct from checkmate       |
| `castling_kingside_works`                       | castling is reachable and applies cleanly  |
| `swap_works` / `same_index_panics`              | the `unsafe` block enforces its contract   |
| `fen_placement_round_trips`                     | FEN parser ↔ serializer are inverses       |
| `opening_pawn_pushes_legal`                     | per-square legal-move query                |
| `promotion_produces_four_choices`               | promotion enumerates Q/R/B/N               |
| `en_passant_capture_works`                      | full en-passant flow works end-to-end      |
| `perft_*_through_depth_3`                       | perft 1-3 (full move-generator correctness)|
| `perft_*_at_depth_4` (`--ignored`)              | perft depth 4 (extended verification)      |
| `rendered_frame_contains_pieces_and_panels`     | the actual UI code path renders correctly  |
| `selecting_pawn_renders_legal_targets`          | piece-selection rendering doesn't crash    |

### Lint hygiene

```bash
cargo clippy --all-features --all-targets -- -D warnings
```

Compiles clean &mdash; zero warnings under the strictest clippy settings,
across all features and all targets (lib, bins, examples, tests).

---

## Architecture


| File                | What lives here                              | Concepts on display                              |
|---------------------|----------------------------------------------|--------------------------------------------------|
| `src/piece.rs`      | `Color`, `PieceKind`, `Piece`                | enums, exhaustive `match`, `Copy` types          |
| `src/moves.rs`      | `Square`, `Move`, `MoveKind`                 | enums *with associated data* (`Promotion {…}`)   |
| `src/board.rs`      | `Grid<const N>`, `Board`, FEN                | const generics, lifetimes, iterator chains       |
| `src/move_gen.rs`   | `PieceLogic` trait + per-piece impls         | trait + generic dispatch, function pointers      |
| `src/game.rs`       | `GameState`, special moves, check/mate       | `Box<HistoryEntry>`, iterator pipelines, `match` |
| `src/ui.rs`         | `ui::draw`, status formatting                | `Cow<'static, str>`, fat pointers                |
| `src/app.rs`        | async event loop, terminal setup             | `Arc<Mutex<…>>`, mpsc channel, `Send`/`Sync`     |
| `src/unsafe_demo.rs`| raw-pointer swap helper                      | `unsafe`, written safety contract                |
| `src/ai.rs` (feat.) | rayon-parallel move suggester                | parallel iterators, `Send` bound                 |

```
chess/
├── Cargo.toml
├── README.md           ← this file
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── piece.rs        ←┐
│   ├── moves.rs        ← │ core types
│   ├── board.rs        ← │
│   ├── move_gen.rs     ← │ rules
│   ├── game.rs         ←┘
│   ├── ui.rs           ←┐
│   ├── app.rs          ← │ runtime + UI
│   ├── unsafe_demo.rs  ← │
│   └── ai.rs           ←┘ (feature-gated)
├── examples/
│   └── perft.rs
└── tests/
    ├── integration.rs
    └── snapshot.rs
```

---

## Crates

| Crate         | What I used it for                                           |
|---------------|--------------------------------------------------------------|
| `ratatui`     | All the terminal rendering. Buffer-based immediate-mode UI. |
| `crossterm`   | Cross-platform raw mode + async `EventStream` for input.     |
| `tokio`       | Async runtime, `select!`, `mpsc::channel`, `Mutex`.          |
| `futures`     | `StreamExt::next` to poll the crossterm event stream.        |
| `anyhow`      | Ergonomic error propagation in the application layer.        |
| `rayon` (opt) | Data parallelism for the optional AI module.                 |

---

## Reflections

A few things I'll take away from this:

- **Pattern matching is the killer feature.** I never had a "did I forget
  the en-passant case?" debugging session. When I added `MoveKind::EnPassant`
  later, the compiler immediately listed every place I'd missed.
- **The borrow checker is a teacher, not an obstacle.** The first time I
  tried to render and mutate the game state from the same task, the
  compiler refused. Wrapping it in `Arc<Mutex<>>` wasn't a workaround &mdash;
  it was the language forcing me to *say* "this is shared mutable state".
- **`unsafe` made me a more careful programmer.** Writing the safety
  contract for `swap_squares_unsafe` made me think harder about *why*
  `i != j` matters than I would have if I'd just used `slice::swap`.
- **The hardest bug was a chess bug, not a Rust bug.** My first
  `is_square_attacked` treated a pawn's forward push as an attack on
  that square &mdash; but pawns attack only diagonally. Caught by perft
  being off at depth 4.

---
