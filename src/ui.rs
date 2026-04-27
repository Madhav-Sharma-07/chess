//! ratatui rendering.
//!
//! Demonstrates: fat pointers (`&str`, `&[T]`), borrowing the
//! `GameState` for *read-only* rendering with a clear lifetime.

use crate::game::{GameState, GameStatus};
use crate::moves::Square;
use crate::piece::Color;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color as TuiColor, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

/// Transient UI state that lives outside the model.
#[derive(Debug, Clone)]
pub struct UiState {
    /// File/rank cursor on the board (0..8 each).
    pub cursor: (i8, i8),
    /// Currently selected square (a piece the user is moving).
    pub selected: Option<Square>,
    /// If `Some`, the pending move requires a promotion choice.
    /// We display a banner asking for q/r/b/n.
    pub pending_promotion_to: Option<Square>,
    /// Last error/status message shown below the board.
    pub message: String,
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}

impl UiState {
    pub fn new() -> Self {
        Self {
            cursor: (4, 1), // start near the e2 pawn
            selected: None,
            pending_promotion_to: None,
            message: "Use arrows to move, Enter to select/move, q to quit, n for new game".into(),
        }
    }

    pub fn cursor_square(&self) -> Square {
        Square::from_coords(self.cursor.0, self.cursor.1).unwrap()
    }
}

/// Top-level draw function.  Borrows both the `GameState` and the
/// `UiState` immutably; the lifetimes are inferred but tied to the
/// frame's lifetime for the rendered widgets.
pub fn draw(f: &mut Frame, game: &GameState, ui: &UiState) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(40), Constraint::Min(20)])
        .split(area);

    draw_board(f, chunks[0], game, ui);
    draw_sidebar(f, chunks[1], game, ui);
}

fn draw_board(f: &mut Frame, area: Rect, game: &GameState, ui: &UiState) {
    // Compute the highlighted "legal move" target squares for the
    // currently selected piece (if any).
    let highlights: Vec<Square> = ui
        .selected
        .map(|sq| {
            game.legal_moves_from(sq)
                .into_iter()
                .map(|m| m.to)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // Build the board as a vector of `Line`s — one per rank, plus a
    // file-letter footer.
    let mut lines: Vec<Line> = Vec::with_capacity(10);
    lines.push(Line::from("   a  b  c  d  e  f  g  h "));
    for rank in (0..8).rev() {
        let mut spans: Vec<Span> = Vec::with_capacity(9);
        spans.push(Span::raw(format!(" {} ", rank + 1)));
        for file in 0..8 {
            let sq = Square::from_coords(file, rank).unwrap();
            let is_light = (file + rank) % 2 == 1;
            let is_cursor = ui.cursor == (file, rank);
            let is_selected = ui.selected == Some(sq);
            let is_highlight = highlights.contains(&sq);

            // Choose background colour. Highlights override the
            // light/dark pattern; cursor + selected get distinct hues.
            let bg = if is_cursor {
                TuiColor::LightBlue
            } else if is_selected {
                TuiColor::LightYellow
            } else if is_highlight {
                TuiColor::LightGreen
            } else if is_light {
                TuiColor::Rgb(180, 160, 130)
            } else {
                TuiColor::Rgb(100, 80, 60)
            };

            let glyph = match game.board.get(sq) {
                Some(p) => format!(" {} ", p.unicode()),
                None => "   ".to_string(),
            };
            let fg = match game.board.get(sq).map(|p| p.color) {
                Some(Color::White) => TuiColor::White,
                Some(Color::Black) => TuiColor::Black,
                None => TuiColor::Reset,
            };
            spans.push(Span::styled(
                glyph,
                Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
            ));
        }
        spans.push(Span::raw(format!(" {}", rank + 1)));
        lines.push(Line::from(spans));
    }
    lines.push(Line::from("   a  b  c  d  e  f  g  h "));

    let block = Block::default().borders(Borders::ALL).title(" Chess ");
    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, area);
}

fn draw_sidebar(f: &mut Frame, area: Rect, game: &GameState, ui: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(5),
        ])
        .split(area);

    // --- status panel --------------------------------------------------
    let status_text = format_status(game);
    let mover = match game.side_to_move {
        Color::White => "White to move",
        Color::Black => "Black to move",
    };
    let status_para = Paragraph::new(vec![
        Line::from(Span::styled(
            mover,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(status_text.as_ref()),
    ])
    .block(Block::default().borders(Borders::ALL).title(" Status "));
    f.render_widget(status_para, chunks[0]);

    // --- move history --------------------------------------------------
    // Build numbered SAN-ish lines. Iterator pipeline: chunks_exact
    // pairs up consecutive half-moves into "1. e4 e5".
    let mut hist_lines: Vec<ListItem> = Vec::new();
    let mut i = 0;
    while i < game.history.len() {
        let white = &game.history[i];
        let black = game.history.get(i + 1);
        let line = match black {
            Some(b) => format!("{}. {}  {}", white.move_number, white.display, b.display),
            None => format!("{}. {}", white.move_number, white.display),
        };
        hist_lines.push(ListItem::new(line));
        i += 2;
    }
    let history = List::new(hist_lines)
        .block(Block::default().borders(Borders::ALL).title(" Moves "));
    f.render_widget(history, chunks[1]);

    // --- message line --------------------------------------------------
    let msg = if ui.pending_promotion_to.is_some() {
        "Promote to: q (Queen)  r (Rook)  b (Bishop)  n (Knight)".to_string()
    } else {
        ui.message.clone()
    };
    let msg_para = Paragraph::new(msg)
        .block(Block::default().borders(Borders::ALL).title(" Info "));
    f.render_widget(msg_para, chunks[2]);
}

/// Small helper that returns either a `&'static str` or an owned
/// `String` depending on the status — a natural fit for `Cow`.
/// We use the `Cow` type from `std::borrow` directly to demonstrate
/// the misc-topics chapter.
pub fn format_status(game: &GameState) -> std::borrow::Cow<'static, str> {
    use std::borrow::Cow;
    match game.status {
        GameStatus::Ongoing => Cow::Borrowed("Game in progress"),
        GameStatus::Check(c) => match c {
            Color::White => Cow::Borrowed("White is in check!"),
            Color::Black => Cow::Borrowed("Black is in check!"),
        },
        GameStatus::Checkmate(loser) => {
            let winner = loser.opponent();
            // Owned because we splice in the winner's name.
            Cow::Owned(format!(
                "Checkmate — {} wins",
                match winner {
                    Color::White => "White",
                    Color::Black => "Black",
                }
            ))
        }
        GameStatus::Stalemate => Cow::Borrowed("Stalemate — draw"),
        GameStatus::Draw(reason) => Cow::Owned(format!("Draw — {reason}")),
    }
}
