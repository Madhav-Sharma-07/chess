//! Piece definitions.
//!
//! Demonstrates: enums, exhaustive pattern matching, `derive` attributes,
//! `Copy`/`Clone` (a "Copy type" because every field is itself `Copy`),
//! and pass-by-value semantics.

use std::fmt;

/// The two players. Stored as a 1-byte enum (cheap, `Copy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    White,
    Black,
}

impl Color {
    /// Return the opposing colour. Pure function, takes `self` by value
    /// because `Color` is `Copy` (no ownership transfer happens).
    pub const fn opponent(self) -> Self {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    /// Direction of pawn movement (`+1` for White, `-1` for Black) in
    /// rank units. Used heavily inside the iterator-based pawn move
    /// generator.
    pub const fn pawn_dir(self) -> i8 {
        match self {
            Color::White => 1,
            Color::Black => -1,
        }
    }

    /// The starting rank (0-indexed) of a pawn for this colour.
    pub const fn pawn_start_rank(self) -> i8 {
        match self {
            Color::White => 1,
            Color::Black => 6,
        }
    }

    /// The promotion rank — the rank a pawn reaches to be promoted.
    pub const fn promotion_rank(self) -> i8 {
        match self {
            Color::White => 7,
            Color::Black => 0,
        }
    }
}

/// One of six chess piece types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl PieceKind {
    /// A short single-character ASCII tag used for FEN serialisation.
    /// White pieces are uppercase; `Color` is applied by the caller.
    pub const fn ascii(self) -> char {
        match self {
            PieceKind::Pawn => 'P',
            PieceKind::Knight => 'N',
            PieceKind::Bishop => 'B',
            PieceKind::Rook => 'R',
            PieceKind::Queen => 'Q',
            PieceKind::King => 'K',
        }
    }

    /// Inverse of [`PieceKind::ascii`] — uppercase letter only.
    pub const fn from_ascii(c: char) -> Option<Self> {
        match c {
            'P' => Some(PieceKind::Pawn),
            'N' => Some(PieceKind::Knight),
            'B' => Some(PieceKind::Bishop),
            'R' => Some(PieceKind::Rook),
            'Q' => Some(PieceKind::Queen),
            'K' => Some(PieceKind::King),
            _ => None,
        }
    }
}

/// A piece on the board. The whole struct is `Copy` because both
/// fields are `Copy` — illustrates the "Copy type" idea from class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

impl Piece {
    pub const fn new(color: Color, kind: PieceKind) -> Self {
        Self { color, kind }
    }

    /// The Unicode chess glyph used in the TUI.
    ///
    /// We use the **shape** of the glyph as the white/black cue —
    /// hollow outline glyphs (U+2654..U+2659) for white pieces, filled
    /// silhouette glyphs (U+265A..U+265F) for black pieces — and a
    /// single bright foreground colour in `ui.rs` for all pieces.
    ///
    /// Why shape-based: a two-shade brown board has *four* piece-on-
    /// square combinations.  Trying to distinguish white from black
    /// purely by foreground colour means at least one combination
    /// always has a low-contrast piece-vs-background pair (e.g. a
    /// yellow-tinted "white" piece on a tan light square is the same
    /// hue family as the square).  By contrast, a hollow-outline
    /// glyph and a filled-silhouette glyph are visually different at
    /// a glance regardless of the background, so the cue survives
    /// any terminal palette quirks.
    pub const fn unicode(self) -> char {
        match (self.color, self.kind) {
            (Color::White, PieceKind::King) => '\u{2654}',
            (Color::White, PieceKind::Queen) => '\u{2655}',
            (Color::White, PieceKind::Rook) => '\u{2656}',
            (Color::White, PieceKind::Bishop) => '\u{2657}',
            (Color::White, PieceKind::Knight) => '\u{2658}',
            (Color::White, PieceKind::Pawn) => '\u{2659}',
            (Color::Black, PieceKind::King) => '\u{265A}',
            (Color::Black, PieceKind::Queen) => '\u{265B}',
            (Color::Black, PieceKind::Rook) => '\u{265C}',
            (Color::Black, PieceKind::Bishop) => '\u{265D}',
            (Color::Black, PieceKind::Knight) => '\u{265E}',
            (Color::Black, PieceKind::Pawn) => '\u{265F}',
        }
    }

    /// FEN character: uppercase for White, lowercase for Black.
    pub fn fen_char(self) -> char {
        let c = self.kind.ascii();
        match self.color {
            Color::White => c,
            Color::Black => c.to_ascii_lowercase(),
        }
    }

    /// Inverse of [`Piece::fen_char`].
    pub fn from_fen_char(c: char) -> Option<Self> {
        let color = if c.is_ascii_uppercase() {
            Color::White
        } else {
            Color::Black
        };
        let kind = PieceKind::from_ascii(c.to_ascii_uppercase())?;
        Some(Piece::new(color, kind))
    }
}

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.unicode())
    }
}
