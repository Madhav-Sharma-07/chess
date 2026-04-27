//! Async event loop and shared state.
//!
//! Demonstrates:
//! * **`Arc<Mutex<T>>`** — interior mutability behind a smart pointer
//!   so the input task and the render loop can both read/write the
//!   `GameState`.
//! * **Message passing** — input events flow over a `tokio::mpsc`
//!   channel from the input task to the main loop. This is the
//!   "MPSC channel" example from the concurrency lecture.
//! * **`Send`/`Sync` bounds** — `Arc<Mutex<GameState>>` is `Send +
//!   Sync` because `GameState: Send`, which lets `tokio::spawn` work.
//! * **Async/await** — the main loop is `async fn`; we `await` both
//!   the channel and a render tick.
//! * **RAII** — the `cleanup_terminal` helper restores the terminal
//!   on drop in case of panics.

use std::io::{self, Stdout};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEventKind,
    KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::{mpsc, Mutex};

use crate::game::GameState;
use crate::moves::{Move, MoveKind, Square};
use crate::piece::{Color, PieceKind};
use crate::ui::{draw, UiState};

/// RAII guard: restores terminal modes on drop, even on panic.
/// Demonstrates the "destructor as cleanup" idiom from the value
/// semantics lecture.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
        Ok(TerminalGuard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Best-effort cleanup: ignore errors so we don't double-panic.
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    }
}

/// Inputs sent from the input task to the main loop.
#[derive(Debug, Clone, Copy)]
enum AppEvent {
    MoveCursor(i8, i8),
    Confirm,
    Cancel,
    Quit,
    NewGame,
    /// Promotion piece choice (q/r/b/n).
    Promote(PieceKind),
    /// Cycle AI opponent: off → AI as Black → AI as White → off.
    /// (When the `ai` feature isn't compiled in we still accept this
    /// variant; the handler shows a gentle "not compiled" message.)
    AiToggle,
    /// Ask the AI for a hint without playing it.
    AiSuggest,
    /// Trigger a redraw because the channel is otherwise idle.
    Tick,
}

/// Public entry point — installs the terminal, spawns the input
/// task, runs the render loop, and tears everything down.
pub async fn run() -> Result<()> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let game = Arc::new(Mutex::new(GameState::new()));
    let mut ui = UiState::new();

    // mpsc channel: input task -> main loop. The buffer is small
    // because the user can't generate events faster than 60 Hz.
    let (tx, mut rx) = mpsc::channel::<AppEvent>(32);

    // Spawn the input task.  It owns its own Sender; when the main
    // loop drops the corresponding Receiver we'll get an error and
    // the task exits cleanly.
    let input_tx = tx.clone();
    let input_handle = tokio::spawn(async move {
        if let Err(e) = input_loop(input_tx).await {
            eprintln!("input loop error: {e}");
        }
    });

    // Initial draw.
    {
        let game_ref = game.lock().await;
        terminal.draw(|f| draw(f, &game_ref, &ui))?;
    }

    // Main render loop.  We use `tokio::select!` to wait on either an
    // input event or a periodic tick (so the screen stays current).
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            ev = rx.recv() => {
                let Some(ev) = ev else { break };
                if matches!(ev, AppEvent::Quit) {
                    break;
                }
                handle_event(ev, &game, &mut ui).await;
            }
            _ = tick.tick() => {
                // No-op; falls through to the redraw below.
            }
        }

        let game_ref = game.lock().await;
        terminal.draw(|f| draw(f, &game_ref, &ui))?;
    }

    // The input task is parked on `EventStream::next().await` waiting
    // for the next keypress; dropping the channel sender alone will
    // not wake it.  Abort it explicitly so the spawned future is
    // cancelled at its next `.await`, then await the JoinHandle so the
    // task is fully torn down before we return (and `TerminalGuard`
    // restores the terminal).
    drop(tx);
    input_handle.abort();
    let _ = input_handle.await;
    Ok(())
}

/// Apply an event to the model + ui state.  Holds the mutex only as
/// briefly as needed (await points are inside the lock so we don't
/// hold it across them).
async fn handle_event(ev: AppEvent, game: &Arc<Mutex<GameState>>, ui: &mut UiState) {
    match ev {
        AppEvent::Tick | AppEvent::Quit => {}
        AppEvent::Cancel => {
            ui.selected = None;
            ui.pending_promotion_to = None;
            ui.message = "Selection cancelled".into();
        }
        AppEvent::MoveCursor(df, dr) => {
            ui.cursor.0 = (ui.cursor.0 + df).clamp(0, 7);
            ui.cursor.1 = (ui.cursor.1 + dr).clamp(0, 7);
        }
        AppEvent::NewGame => {
            let mut g = game.lock().await;
            *g = GameState::new();
            ui.selected = None;
            ui.pending_promotion_to = None;
            ui.message = "New game!".into();
            // If the AI is configured to play White, it moves first.
            maybe_play_ai_move(&mut g, ui);
        }
        AppEvent::Confirm => {
            let mut g = game.lock().await;
            let cur = ui.cursor_square();
            match ui.selected {
                None => {
                    // First click: pick up a piece if it's ours.
                    match g.board.get(cur) {
                        Some(p) if p.color == g.side_to_move => {
                            ui.selected = Some(cur);
                            ui.message = format!("Selected {}", cur);
                        }
                        Some(_) => {
                            ui.message = "Not your piece".into();
                        }
                        None => {
                            ui.message = "Empty square".into();
                        }
                    }
                }
                Some(from) => {
                    if from == cur {
                        ui.selected = None;
                        ui.message = "Deselected".into();
                        return;
                    }
                    // Look up legal moves from this square; pick the one
                    // ending at the cursor.
                    let candidates: Vec<Move> = g.legal_moves_from(from);
                    let chosen = candidates.iter().copied().find(|m| m.to == cur);

                    match chosen {
                        Some(m) if matches!(m.kind, MoveKind::Promotion { .. }) => {
                            // Defer to a follow-up key press.
                            ui.pending_promotion_to = Some(m.to);
                            ui.message = "Choose promotion piece".into();
                        }
                        Some(m) => {
                            apply_chosen(&mut g, m, ui);
                            // If the AI is on, it now responds.
                            maybe_play_ai_move(&mut g, ui);
                        }
                        None => {
                            ui.message = "Illegal move".into();
                        }
                    }
                }
            }
        }
        AppEvent::Promote(kind) => {
            let Some(to) = ui.pending_promotion_to else { return };
            let Some(from) = ui.selected else { return };
            let mv = Move::new(
                from,
                to,
                MoveKind::Promotion {
                    promote_to: kind,
                    capture: false, // make_move re-resolves against legal moves
                },
            );
            let mut g = game.lock().await;
            apply_chosen(&mut g, mv, ui);
            // AI responds after a promotion move just like any other.
            maybe_play_ai_move(&mut g, ui);
        }
        AppEvent::AiToggle => handle_ai_toggle(game, ui).await,
        AppEvent::AiSuggest => handle_ai_suggest(game, ui).await,
    }
}

/// AI-toggle handler.  Cycles `ui.ai_side` through `None →
/// Some(Black) → Some(White) → None` and immediately plays the AI's
/// move if it's now its turn.  When the `ai` feature is off this is
/// just a friendly message — we don't pretend the toggle worked.
#[cfg(feature = "ai")]
async fn handle_ai_toggle(game: &Arc<Mutex<GameState>>, ui: &mut UiState) {
    ui.ai_side = match ui.ai_side {
        None => Some(Color::Black),
        Some(Color::Black) => Some(Color::White),
        Some(Color::White) => None,
    };
    let label = match ui.ai_side {
        None => "AI off".to_string(),
        Some(Color::White) => "AI plays White".to_string(),
        Some(Color::Black) => "AI plays Black".to_string(),
    };
    let mut g = game.lock().await;
    ui.message = label;
    // If the toggle landed us on AI's move, play it right away.
    maybe_play_ai_move(&mut g, ui);
}

#[cfg(not(feature = "ai"))]
async fn handle_ai_toggle(_: &Arc<Mutex<GameState>>, ui: &mut UiState) {
    ui.message = "AI not compiled. Rebuild with --features ai.".into();
}

/// AI-suggest handler.  Computes the AI's best move from the current
/// position and shows it in the message bar; does *not* play it.
#[cfg(feature = "ai")]
async fn handle_ai_suggest(game: &Arc<Mutex<GameState>>, ui: &mut UiState) {
    let g = game.lock().await;
    ui.message = match crate::ai::best_move_parallel(&g) {
        Some(mv) => format!("AI suggests {}", mv.long_algebraic()),
        None => "No legal moves (game over)".into(),
    };
}

#[cfg(not(feature = "ai"))]
async fn handle_ai_suggest(_: &Arc<Mutex<GameState>>, ui: &mut UiState) {
    ui.message = "AI not compiled. Rebuild with --features ai.".into();
}

/// Apply `mv`, update UI feedback.
fn apply_chosen(g: &mut GameState, mv: Move, ui: &mut UiState) {
    match g.make_move(mv) {
        Ok(applied) => {
            ui.selected = None;
            ui.pending_promotion_to = None;
            ui.message = format!("Played {}", applied.long_algebraic());
        }
        Err(e) => {
            ui.message = e.to_string();
        }
    }
}

/// If the AI is configured to play the current side and the game is
/// still in progress, ask the AI for a move and apply it.  Called
/// after every event that *might* leave the AI on move (a human move,
/// a new game, a toggle).
///
/// This is feature-gated; in non-AI builds it's a no-op.
#[cfg(feature = "ai")]
fn maybe_play_ai_move(g: &mut GameState, ui: &mut UiState) {
    let Some(ai_side) = ui.ai_side else { return };
    if g.side_to_move != ai_side {
        return;
    }
    // If there are no legal moves the game is over (mate or
    // stalemate) and `best_move_parallel` returns `None`.
    let Some(ai_mv) = crate::ai::best_move_parallel(g) else {
        return;
    };
    if let Ok(applied) = g.make_move(ai_mv) {
        ui.message = format!("AI played {}", applied.long_algebraic());
    }
}

#[cfg(not(feature = "ai"))]
fn maybe_play_ai_move(_: &mut GameState, _: &mut UiState) {}

/// Reads from crossterm's async event stream and forwards mapped
/// events to the main loop.  When the channel closes we exit.
async fn input_loop(tx: mpsc::Sender<AppEvent>) -> Result<()> {
    let mut stream = EventStream::new();
    while let Some(ev) = stream.next().await {
        let ev = ev?;
        let Some(app_event) = translate_event(ev) else {
            continue;
        };
        if tx.send(app_event).await.is_err() {
            break;
        }
    }
    Ok(())
}

/// Map a low-level `crossterm::Event` to our `AppEvent`.  Pure
/// function (no I/O), perfect for unit testing.
fn translate_event(ev: Event) -> Option<AppEvent> {
    let Event::Key(k) = ev else { return None };
    if k.kind == KeyEventKind::Release {
        return None;
    }
    // Ctrl+C exits immediately.
    if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
        return Some(AppEvent::Quit);
    }
    Some(match k.code {
        KeyCode::Char('q') | KeyCode::Esc => AppEvent::Quit,
        KeyCode::Char('n') => AppEvent::NewGame,
        KeyCode::Char(' ') | KeyCode::Enter => AppEvent::Confirm,
        KeyCode::Backspace => AppEvent::Cancel,
        KeyCode::Up => AppEvent::MoveCursor(0, 1),
        KeyCode::Down => AppEvent::MoveCursor(0, -1),
        KeyCode::Left => AppEvent::MoveCursor(-1, 0),
        KeyCode::Right => AppEvent::MoveCursor(1, 0),
        // AI controls (work whether or not the `ai` feature is on;
        // the handler shows a clear message if it isn't).
        KeyCode::Char('a') => AppEvent::AiToggle,
        KeyCode::Char('s') => AppEvent::AiSuggest,
        // Promotion keys: only meaningful after a pawn-to-back-rank
        // candidate move; the main loop checks `pending_promotion_to`.
        KeyCode::Char('Q') => AppEvent::Promote(PieceKind::Queen),
        KeyCode::Char('R') => AppEvent::Promote(PieceKind::Rook),
        KeyCode::Char('B') => AppEvent::Promote(PieceKind::Bishop),
        KeyCode::Char('N') => AppEvent::Promote(PieceKind::Knight),
        _ => AppEvent::Tick,
    })
}

/// Compile-time assertion: `GameState` is `Send + Sync` so it can
/// safely be parked behind an `Arc<Mutex<>>` shared across tasks.
/// This is the same trick the standard library uses for `static`
/// channel asserts. Demonstrates the "Send/Sync" topic.
#[allow(dead_code)]
fn assert_send_sync_state() {
    fn assert<T: Send + Sync>() {}
    assert::<GameState>();
    assert::<Arc<Mutex<GameState>>>();
}

// Keep a couple of imports used only for the assertion above.
#[allow(dead_code)]
fn _keep_alive(_: Stdout, _: Color, _: Square) {}
