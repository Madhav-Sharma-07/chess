//! Move and square representations.
//!
//! Demonstrates: tuple-like structs, `enum` variants with associated
//! data (the `MoveKind` discriminator), exhaustive pattern matching,
//! and small `Copy` types (every value here fits in a few bytes).

use crate::piece::PieceKind;
use std::fmt;

/// Number of squares on one side of the board. Used as a `const` so it
/// can be passed to const generic parameters elsewhere.
pub const BOARD_SIZE: usize = 8;

/// A square on the board, expressed as a 0..64 index.
///
/// Indexing convention: `index = rank * 8 + file`, where rank 0 is
/// White's back rank and file 0 is the a-file.  This keeps everything
/// `Copy` and avoids heap allocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Square(pub u8);

impl Square {
    /// Build a square from `(file, rank)` where both are 0..8.
    /// Returns `None` if either coordinate is out of range.
    pub const fn from_coords(file: i8, rank: i8) -> Option<Self> {
        if file < 0 || file >= 8 || rank < 0 || rank >= 8 {
            None
        } else {
            Some(Square((rank as u8) * 8 + file as u8))
        }
    }

    pub const fn file(self) -> i8 {
        (self.0 % 8) as i8
    }
    pub const fn rank(self) -> i8 {
        (self.0 / 8) as i8
    }

    /// Convert to algebraic notation, e.g. `Square(0) -> "a1"`.
    pub fn algebraic(self) -> String {
        let f = (b'a' + self.file() as u8) as char;
        let r = (b'1' + self.rank() as u8) as char;
        format!("{f}{r}")
    }

    /// Parse `"e4"` style algebraic into a `Square`.
    pub fn parse(s: &str) -> Option<Self> {
        let mut chars = s.chars();
        let file_c = chars.next()?;
        let rank_c = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        let file = (file_c.to_ascii_lowercase() as i8) - b'a' as i8;
        let rank = (rank_c as i8) - b'1' as i8;
        Self::from_coords(file, rank)
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.algebraic())
    }
}

/// Different *kinds* of moves. Modelled as an `enum` with associated
/// data so we can exhaustively `match` and react to each kind during
/// move execution. This is the "Move enum" the proposal mentions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveKind {
    /// A regular non-capturing move.
    Quiet,
    /// A standard capture (the captured piece is on the destination).
    Capture,
    /// A pawn double-step from the starting rank.  The square skipped
    /// over becomes a temporary "en passant target".
    DoublePawnPush,
    /// En-passant capture. The captured pawn is *behind* the target
    /// square, not on it.
    EnPassant,
    /// Castling.  `Kingside` = O-O, `Queenside` = O-O-O.
    CastleKingside,
    CastleQueenside,
    /// Promotion. May or may not also be a capture; we keep a flag.
    /// The promoted-to piece is always non-King, non-Pawn.
    Promotion {
        promote_to: PieceKind,
        capture: bool,
    },
}

/// A fully-qualified move from one square to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Move {
    pub from: Square,
    pub to: Square,
    pub kind: MoveKind,
}

impl Move {
    pub const fn new(from: Square, to: Square, kind: MoveKind) -> Self {
        Self { from, to, kind }
    }

    /// Long algebraic representation, e.g. `e2e4`, `e7e8q`, `O-O`.
    /// Used by the move-history display in the TUI.
    pub fn long_algebraic(self) -> String {
        match self.kind {
            MoveKind::CastleKingside => "O-O".to_string(),
            MoveKind::CastleQueenside => "O-O-O".to_string(),
            MoveKind::Promotion { promote_to, .. } => {
                let p = promote_to.ascii().to_ascii_lowercase();
                format!("{}{}{}", self.from, self.to, p)
            }
            _ => format!("{}{}", self.from, self.to),
        }
    }
}
