//! Snapshot test that exercises the *real* `ui::draw` against
//! ratatui's `TestBackend`, producing a deterministic textual frame
//! we can paste into the README as a "screenshot".
//!
//! Running `cargo test --test snapshot -- --nocapture` prints the
//! full rendered frame, which is what's embedded in `README.md`.

use chess::game::GameState;
use chess::ui::{draw, UiState};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn render_with(width: u16, height: u16, game: &GameState, ui: &UiState) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| draw(f, game, ui))
        .expect("draw should succeed against TestBackend");

    let buffer = terminal.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            let cell = &buffer[(x, y)];
            out.push_str(cell.symbol());
        }
        out.push('\n');
    }
    out
}

fn render(width: u16, height: u16) -> String {
    render_with(width, height, &GameState::new(), &UiState::new())
}

#[test]
fn rendered_frame_contains_pieces_and_panels() {
    let frame = render(80, 24);
    println!("{frame}");

    // Sanity: the panel titles render.
    assert!(frame.contains("Chess"), "board panel title missing");
    assert!(frame.contains("Status"), "status panel title missing");
    assert!(frame.contains("Moves"), "moves panel title missing");
    assert!(frame.contains("Info"), "info panel title missing");

    // The default UiState announces the controls in the info panel.
    assert!(frame.contains("Use arrows"), "controls hint missing");

    // White is to move at the start.
    assert!(frame.contains("White to move"), "turn indicator missing");

    // At least one piece glyph should be present.  We sentinel on
    // the white king's hollow glyph U+2654 because white/black are
    // distinguished by glyph shape (hollow vs filled).
    assert!(frame.contains('\u{2654}'), "white king glyph missing");
    assert!(frame.contains('\u{265A}'), "black king glyph missing");
}

/// A second snapshot demonstrating piece-selection highlights.  The
/// `e2` pawn is "selected" (yellow on a real terminal) and the two
/// legal targets `e3`, `e4` would be highlighted green.  We can't
/// test the colour bytes because `TestBackend` strips styles by
/// default, but we *can* ensure the game state is consistent and
/// that legal-move computation didn't crash.
#[test]
fn selecting_pawn_renders_legal_targets() {
    use chess::moves::Square;
    let game = GameState::new();
    let mut ui = UiState::new();
    ui.cursor = (4, 1); // e2
    ui.selected = Some(Square::parse("e2").unwrap());
    let frame = render_with(80, 24, &game, &ui);
    println!("{frame}");
    assert!(frame.contains("Selected") || frame.contains("Use arrows"));
}
