//! Top-level game state and rules engine.
//!
//! Demonstrates:
//! * **Smart pointers / interior mutability indirectly** — see
//!   `app.rs` where the whole `GameState` is wrapped in
//!   `Arc<Mutex<GameState>>`.
//! * **Iterators + closures** for legal-move filtering.
//! * **Pattern matching** on `MoveKind` during move execution.
//! * **Lifetime elision** in helper methods that return references
//!   into `&self`.
//! * **`Result` / error propagation** for input parsing.
//! * **`Box<dyn ...>` for trait objects** — the move-history is
//!   stored as `Vec<Box<HistoryEntry>>` to demonstrate boxing
//!   (note: `HistoryEntry` is `Sized`, so `Box` is purely a teaching
//!   choice here, not a necessity).

use crate::board::{Board, CastlingRights};
use crate::move_gen::{
    castling_moves, en_passant_moves, generate_pseudo_legal_for_piece,
    is_square_attacked,
};
use crate::moves::{Move, MoveKind, Square};
use crate::piece::{Color, Piece, PieceKind};

/// Outcome of the game in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameStatus {
    Ongoing,
    Check(Color),
    Checkmate(Color), // colour that *got* checkmated (lost)
    Stalemate,
    /// Insufficient material / 50-move / 75-move auto-draw.
    Draw(&'static str),
}

/// One row in the move history. Stored boxed in `GameState::history`
/// to demonstrate `Box`/heap allocation.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub mv: Move,
    /// Algebraic-style display (e.g. `e2e4`).
    pub display: String,
    /// Snapshot of the move number when this entry was made
    /// (1 + halfmove count >> 1).
    pub move_number: u32,
}

/// The full state of a chess game.
#[derive(Debug, Clone)]
pub struct GameState {
    pub board: Board,
    pub side_to_move: Color,
    pub castling: CastlingRights,
    pub en_passant: Option<Square>,
    pub halfmove_clock: u32,
    pub fullmove_number: u32,
    pub history: Vec<Box<HistoryEntry>>,
    pub status: GameStatus,
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

impl GameState {
    pub fn new() -> Self {
        Self {
            board: Board::starting_position(),
            side_to_move: Color::White,
            castling: CastlingRights::ALL,
            en_passant: None,
            halfmove_clock: 0,
            fullmove_number: 1,
            history: Vec::new(),
            status: GameStatus::Ongoing,
        }
    }

    /// Generate every legal move for the side to move.
    /// Implementation: collect pseudo-legal moves, then filter out any
    /// move that leaves the king in check.  This is the
    /// iterator-pipeline approach the proposal advertised.
    pub fn legal_moves(&self) -> Vec<Move> {
        let me = self.side_to_move;
        let mut pseudo: Vec<Move> = Vec::with_capacity(64);

        // Pseudo-legal moves for every piece of mine.
        for (sq, piece) in self.board.pieces_of(me) {
            generate_pseudo_legal_for_piece(&self.board, piece, sq, &mut pseudo);
        }
        // En-passant captures.
        en_passant_moves(&self.board, me, self.en_passant, &mut pseudo);
        // Castling — but only if the king isn't in check right now.
        if let Some(king_sq) = self.board.king_square(me) {
            if !is_square_attacked(&self.board, king_sq, me.opponent()) {
                castling_moves(&self.board, me, self.castling, &mut pseudo);
            }
        }

        // Filter: keep only moves that don't leave our king in check.
        // The closure clones the state, plays the move on the clone,
        // and asks "is my king attacked now?".
        pseudo
            .into_iter()
            .filter(|mv| {
                let mut probe = self.clone();
                // `apply_move_unchecked` ignores legality; we rely on
                // the pseudo-legal generator never producing
                // structurally invalid moves.
                probe.apply_move_unchecked(*mv);
                if let Some(king_sq) = probe.board.king_square(me) {
                    !is_square_attacked(&probe.board, king_sq, me.opponent())
                } else {
                    false // own king disappeared — illegal
                }
            })
            .collect()
    }

    /// Legal moves *originating from a specific square* — used by the
    /// UI to highlight squares.
    pub fn legal_moves_from(&self, from: Square) -> Vec<Move> {
        self.legal_moves()
            .into_iter()
            .filter(|m| m.from == from)
            .collect()
    }

    /// Try to play a move.  Returns the parsed move on success.
    /// The move is matched against the legal-move list, so callers
    /// can pass either the exact move *or* a `Move` whose `kind` is
    /// only roughly correct (e.g. `Quiet` for what's actually a
    /// `Capture`); we'll find the matching legal entry.
    pub fn make_move(&mut self, candidate: Move) -> Result<Move, String> {
        // Find the legal move that has the same from/to (and matching
        // promotion choice if applicable).
        let chosen = self
            .legal_moves()
            .into_iter()
            .find(|m| {
                m.from == candidate.from
                    && m.to == candidate.to
                    && match (m.kind, candidate.kind) {
                        (
                            MoveKind::Promotion { promote_to: a, .. },
                            MoveKind::Promotion { promote_to: b, .. },
                        ) => a == b,
                        // Any other kind: from/to alone disambiguates.
                        _ => !matches!(m.kind, MoveKind::Promotion { .. }),
                    }
            })
            .ok_or_else(|| format!("illegal move {}", candidate.long_algebraic()))?;

        let display = chosen.long_algebraic();
        let move_number = self.fullmove_number;
        self.apply_move_unchecked(chosen);
        self.history
            .push(Box::new(HistoryEntry { mv: chosen, display, move_number }));
        self.recompute_status();
        Ok(chosen)
    }

    /// Apply a move *without* checking legality. Used both by the
    /// public `make_move` (after legality has been verified) and by
    /// the legality filter (on a cloned state).
    fn apply_move_unchecked(&mut self, mv: Move) {
        let mover_color = self.side_to_move;
        let piece = self
            .board
            .get(mv.from)
            .expect("move-gen produced a move from an empty square");

        // Reset en-passant by default; only DoublePawnPush sets it.
        let prev_ep = self.en_passant;
        self.en_passant = None;

        // 50-move rule: reset on pawn move or any capture.
        let captured = self.board.get(mv.to).is_some();
        let resets_clock = piece.kind == PieceKind::Pawn || captured;
        if resets_clock {
            self.halfmove_clock = 0;
        } else {
            self.halfmove_clock += 1;
        }

        match mv.kind {
            MoveKind::Quiet | MoveKind::Capture => {
                self.board.set(mv.to, Some(piece));
                self.board.set(mv.from, None);
            }
            MoveKind::DoublePawnPush => {
                self.board.set(mv.to, Some(piece));
                self.board.set(mv.from, None);
                // The square *behind* the pawn becomes the e.p. target.
                let ep_rank = (mv.from.rank() + mv.to.rank()) / 2;
                self.en_passant = Square::from_coords(mv.from.file(), ep_rank);
            }
            MoveKind::EnPassant => {
                // Move the pawn.
                self.board.set(mv.to, Some(piece));
                self.board.set(mv.from, None);
                // Captured pawn is on the same file as `mv.to` but on
                // the rank we moved *from*.
                let cap_sq =
                    Square::from_coords(mv.to.file(), mv.from.rank()).unwrap();
                self.board.set(cap_sq, None);
                // For sanity, track the previous en-passant target
                // for debugging; not used further.
                let _ = prev_ep;
            }
            MoveKind::CastleKingside => {
                let rank = mv.from.rank();
                let king_to = Square::from_coords(6, rank).unwrap();
                let rook_from = Square::from_coords(7, rank).unwrap();
                let rook_to = Square::from_coords(5, rank).unwrap();
                self.board.set(king_to, Some(piece));
                self.board.set(mv.from, None);
                let rook = self
                    .board
                    .get(rook_from)
                    .expect("castling without rook is impossible");
                self.board.set(rook_to, Some(rook));
                self.board.set(rook_from, None);
            }
            MoveKind::CastleQueenside => {
                let rank = mv.from.rank();
                let king_to = Square::from_coords(2, rank).unwrap();
                let rook_from = Square::from_coords(0, rank).unwrap();
                let rook_to = Square::from_coords(3, rank).unwrap();
                self.board.set(king_to, Some(piece));
                self.board.set(mv.from, None);
                let rook = self
                    .board
                    .get(rook_from)
                    .expect("castling without rook is impossible");
                self.board.set(rook_to, Some(rook));
                self.board.set(rook_from, None);
            }
            MoveKind::Promotion { promote_to, .. } => {
                self.board
                    .set(mv.to, Some(Piece::new(mover_color, promote_to)));
                self.board.set(mv.from, None);
            }
        }

        // Update castling rights based on the move.
        self.update_castling_rights_after(mv, piece);

        // Switch side and bump fullmove counter (after Black moves).
        if mover_color == Color::Black {
            self.fullmove_number += 1;
        }
        self.side_to_move = mover_color.opponent();
    }

    /// Lose castling rights when the king or rooks move (or rooks are
    /// captured). Pure helper, no side effects beyond `self.castling`.
    fn update_castling_rights_after(&mut self, mv: Move, piece: Piece) {
        // King moved or castled: lose both rights for that colour.
        if piece.kind == PieceKind::King {
            match piece.color {
                Color::White => {
                    self.castling.remove(CastlingRights::WHITE_KING);
                    self.castling.remove(CastlingRights::WHITE_QUEEN);
                }
                Color::Black => {
                    self.castling.remove(CastlingRights::BLACK_KING);
                    self.castling.remove(CastlingRights::BLACK_QUEEN);
                }
            }
        }
        // Rook moved from its home square.
        if piece.kind == PieceKind::Rook {
            match (piece.color, mv.from.file(), mv.from.rank()) {
                (Color::White, 0, 0) => self.castling.remove(CastlingRights::WHITE_QUEEN),
                (Color::White, 7, 0) => self.castling.remove(CastlingRights::WHITE_KING),
                (Color::Black, 0, 7) => self.castling.remove(CastlingRights::BLACK_QUEEN),
                (Color::Black, 7, 7) => self.castling.remove(CastlingRights::BLACK_KING),
                _ => {}
            }
        }
        // Rook captured on its home square — losing rights.
        match (mv.to.file(), mv.to.rank()) {
            (0, 0) => self.castling.remove(CastlingRights::WHITE_QUEEN),
            (7, 0) => self.castling.remove(CastlingRights::WHITE_KING),
            (0, 7) => self.castling.remove(CastlingRights::BLACK_QUEEN),
            (7, 7) => self.castling.remove(CastlingRights::BLACK_KING),
            _ => {}
        }
    }

    /// Recompute `self.status` based on the current position.
    fn recompute_status(&mut self) {
        let me = self.side_to_move;
        let in_check = self
            .board
            .king_square(me)
            .map(|k| is_square_attacked(&self.board, k, me.opponent()))
            .unwrap_or(false);
        let any_legal = !self.legal_moves().is_empty();

        self.status = match (in_check, any_legal) {
            (true, false) => GameStatus::Checkmate(me),
            (false, false) => GameStatus::Stalemate,
            (true, true) => GameStatus::Check(me),
            (false, true) => {
                if self.halfmove_clock >= 100 {
                    GameStatus::Draw("50-move rule")
                } else if !self.has_sufficient_material() {
                    GameStatus::Draw("insufficient material")
                } else {
                    GameStatus::Ongoing
                }
            }
        };
    }

    /// Detect the obvious "can't possibly checkmate" cases.
    /// (KvK, KvK+N, KvK+B; everything else is treated as sufficient.)
    fn has_sufficient_material(&self) -> bool {
        let mut minors_w = 0;
        let mut minors_b = 0;
        let mut other = false;
        for (_, p) in self.board.grid.iter_pieces() {
            match p.kind {
                PieceKind::King => {}
                PieceKind::Knight | PieceKind::Bishop => match p.color {
                    Color::White => minors_w += 1,
                    Color::Black => minors_b += 1,
                },
                _ => other = true,
            }
        }
        if other {
            return true;
        }
        // No major pieces & at most one minor per side ⇒ insufficient.
        !(minors_w <= 1 && minors_b <= 1)
    }
}

// ---------------------------------------------------------------------------
//  Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_position_has_20_moves() {
        let g = GameState::new();
        assert_eq!(g.legal_moves().len(), 20);
    }

    #[test]
    fn fools_mate_is_mate() {
        let mut g = GameState::new();
        // 1. f3 e5  2. g4 Qh4#
        play(&mut g, "f2f3");
        play(&mut g, "e7e5");
        play(&mut g, "g2g4");
        play(&mut g, "d8h4");
        assert!(matches!(g.status, GameStatus::Checkmate(Color::White)));
    }

    #[test]
    fn stalemate_detected() {
        // Classic K+Q stalemate: black king on a8, white queen on c7,
        // white king on c6, black to move.
        let placement = "k7/2Q5/2K5/8/8/8/8/8";
        let board = Board::from_fen(placement).unwrap();
        let g = GameState {
            board,
            side_to_move: Color::Black,
            castling: CastlingRights::default(),
            en_passant: None,
            halfmove_clock: 0,
            fullmove_number: 1,
            history: Vec::new(),
            status: GameStatus::Ongoing,
        };
        // Recompute status via a no-op clone path:
        let mut g2 = g.clone();
        g2.recompute_status();
        assert_eq!(g2.status, GameStatus::Stalemate);
    }

    #[test]
    fn castling_kingside_works() {
        let mut g = GameState::new();
        // Clear path for white kingside castle.
        play(&mut g, "g1f3");
        play(&mut g, "g8f6");
        play(&mut g, "g2g3");
        play(&mut g, "g7g6");
        play(&mut g, "f1g2");
        play(&mut g, "f8g7");
        // Now O-O legal.
        let castle = g
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m.kind, MoveKind::CastleKingside))
            .expect("kingside castle should be legal here");
        g.make_move(castle).unwrap();
    }

    fn play(g: &mut GameState, s: &str) {
        let from = Square::parse(&s[0..2]).unwrap();
        let to = Square::parse(&s[2..4]).unwrap();
        let mv = Move::new(from, to, MoveKind::Quiet);
        g.make_move(mv).expect("test move should be legal");
    }
}
