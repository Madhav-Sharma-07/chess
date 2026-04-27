//! The `Board` data structure.
//!
//! Demonstrates:
//! * **Const generics** — the `Grid<N>` helper is parameterised on the
//!   board side length, so it stores exactly `N*N` squares with no
//!   heap allocation.
//! * **Lifetimes** — `iter_pieces` returns an iterator that borrows
//!   from the board; the explicit `'a` makes the borrow visible.
//! * **Pattern matching** + small algorithm for FEN parsing.
//! * **`Copy` of small fixed-size arrays** at the type level — the
//!   board is `Clone` (and cheap to clone) so move-make/unmake can
//!   work on snapshots.

use crate::moves::{Square, BOARD_SIZE};
use crate::piece::{Color, Piece, PieceKind};

/// Generic fixed-size grid: `N` squares per side.  Using a const
/// generic here keeps the data flat and stack-friendly.
///
/// We store the grid as `[[T; N]; N]` (rather than `[T; N*N]`) so the
/// expression compiles on stable Rust without
/// `feature(generic_const_exprs)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid<const N: usize> {
    cells: [[Option<Piece>; N]; N],
}

impl<const N: usize> Grid<N> {
    pub const SIDE: usize = N;

    pub fn empty() -> Self {
        Self {
            cells: [[None; N]; N],
        }
    }

    /// Read-only access — note the elided lifetime: the returned
    /// value is `Copy` (`Option<Piece>`), so no borrow leaks out.
    pub fn get(&self, sq: Square) -> Option<Piece> {
        self.cells[sq.rank() as usize][sq.file() as usize]
    }

    pub fn set(&mut self, sq: Square, piece: Option<Piece>) {
        self.cells[sq.rank() as usize][sq.file() as usize] = piece;
    }

    /// Iterate over `(Square, Piece)` pairs for occupied squares.
    /// The explicit lifetime `'a` ties the iterator's lifetime to the
    /// board borrow it was created from.
    pub fn iter_pieces<'a>(&'a self) -> impl Iterator<Item = (Square, Piece)> + 'a {
        self.cells.iter().enumerate().flat_map(|(rank, row)| {
            row.iter().enumerate().filter_map(move |(file, slot)| {
                slot.map(|p| (Square::from_coords(file as i8, rank as i8).unwrap(), p))
            })
        })
    }
}

/// Castling rights stored as a 4-bit bitset.  Each method is `const`
/// so the compiler can fold them at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CastlingRights(pub u8);

impl CastlingRights {
    pub const WHITE_KING: u8 = 0b0001;
    pub const WHITE_QUEEN: u8 = 0b0010;
    pub const BLACK_KING: u8 = 0b0100;
    pub const BLACK_QUEEN: u8 = 0b1000;
    pub const ALL: Self = Self(0b1111);

    pub const fn has(self, mask: u8) -> bool {
        (self.0 & mask) != 0
    }
    pub fn add(&mut self, mask: u8) {
        self.0 |= mask;
    }
    pub fn remove(&mut self, mask: u8) {
        self.0 &= !mask;
    }
}

/// The board *position* — pieces only.  Higher-level state (turn,
/// castling, en-passant target, halfmove counters) lives in
/// [`crate::game::GameState`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub grid: Grid<BOARD_SIZE>,
}

impl Board {
    /// The standard chess starting position.
    pub fn starting_position() -> Self {
        // FEN for the initial position. Calling `from_fen` keeps the
        // construction logic in one place and exercises the parser.
        Self::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR")
            .expect("starting FEN is hard-coded and known to be valid")
    }

    pub fn empty() -> Self {
        Self {
            grid: Grid::empty(),
        }
    }

    pub fn get(&self, sq: Square) -> Option<Piece> {
        self.grid.get(sq)
    }

    pub fn set(&mut self, sq: Square, piece: Option<Piece>) {
        self.grid.set(sq, piece);
    }

    /// Find the king of the given colour. Returns `None` only on a
    /// malformed test position.
    pub fn king_square(&self, color: Color) -> Option<Square> {
        self.grid.iter_pieces().find_map(|(sq, p)| {
            if p.color == color && p.kind == PieceKind::King {
                Some(sq)
            } else {
                None
            }
        })
    }

    /// Iterate over all pieces of one colour. Demonstrates an
    /// iterator pipeline using `filter` + a closure.
    pub fn pieces_of<'a>(&'a self, color: Color) -> impl Iterator<Item = (Square, Piece)> + 'a {
        self.grid
            .iter_pieces()
            .filter(move |(_, p)| p.color == color)
    }

    /// Parse the *piece-placement* portion of a FEN string (the bit
    /// before the first space).  We support the full 6-field FEN in
    /// [`crate::game::GameState::from_fen`]; this is just the board.
    pub fn from_fen(placement: &str) -> Result<Self, String> {
        let mut board = Board::empty();
        // FEN ranks come 8-down-to-1, so rank index 7 first.
        let mut rank: i8 = 7;
        for row in placement.split('/') {
            let mut file: i8 = 0;
            for c in row.chars() {
                if let Some(d) = c.to_digit(10) {
                    file += d as i8;
                } else {
                    let piece = Piece::from_fen_char(c)
                        .ok_or_else(|| format!("bad FEN char '{c}'"))?;
                    let sq = Square::from_coords(file, rank)
                        .ok_or_else(|| format!("FEN out of bounds at {file},{rank}"))?;
                    board.set(sq, Some(piece));
                    file += 1;
                }
            }
            if file != 8 {
                return Err(format!("FEN row '{row}' did not cover 8 files"));
            }
            rank -= 1;
        }
        if rank != -1 {
            return Err("FEN did not contain 8 ranks".to_string());
        }
        Ok(board)
    }

    /// Serialise the placement portion back to FEN.  Used both for
    /// logging and (eventually) save/load.
    pub fn to_fen_placement(&self) -> String {
        let mut out = String::with_capacity(64);
        for rank in (0..8).rev() {
            let mut empty: u8 = 0;
            for file in 0..8 {
                let sq = Square::from_coords(file, rank).unwrap();
                match self.get(sq) {
                    Some(p) => {
                        if empty > 0 {
                            out.push((b'0' + empty) as char);
                            empty = 0;
                        }
                        out.push(p.fen_char());
                    }
                    None => empty += 1,
                }
            }
            if empty > 0 {
                out.push((b'0' + empty) as char);
            }
            if rank != 0 {
                out.push('/');
            }
        }
        out
    }
}
