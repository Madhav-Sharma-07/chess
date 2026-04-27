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

    // Drop sender so input loop exits if it hasn't already.
    drop(tx);
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
        }
    }
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
