//! Optional AI suggester (stretch goal from the proposal).
//!
//! Compiled only when `--features ai` is passed.  Demonstrates:
//! * **`Send`/`Sync` bounds** — `rayon::par_iter` requires the closure
//!   to be `Send`, which it is because `GameState: Send + Sync`.
//! * **Parallel iterators** — `into_par_iter().map(...).max_by_key(...)`
//!   replaces a sequential `iter()` for free.
//! * **Closures + iterators** as zero-cost abstractions — the body of
//!   the closure is monomorphised at the call site.
//!
//! The "AI" is intentionally trivial (1-ply material evaluation):
//! the point is the parallel pipeline, not chess strength.

use crate::game::GameState;
use crate::moves::Move;
use crate::piece::{Color, Piece, PieceKind};

use rayon::prelude::*;

/// Tiny material evaluator.
fn material(state: &GameState, side: Color) -> i32 {
    state
        .board
        .grid
        .iter_pieces()
        .map(|(_, p)| signed_value(p, side))
        .sum()
}

fn signed_value(p: Piece, side: Color) -> i32 {
    let v = match p.kind {
        PieceKind::Pawn => 100,
        PieceKind::Knight | PieceKind::Bishop => 300,
        PieceKind::Rook => 500,
        PieceKind::Queen => 900,
        PieceKind::King => 0,
    };
    if p.color == side { v } else { -v }
}

/// Pick the move that maximises our material after one ply, in
/// parallel using rayon. Returns `None` if there are no legal moves.
pub fn best_move_parallel(state: &GameState) -> Option<Move> {
    let me = state.side_to_move;
    let moves = state.legal_moves();
    moves
        .into_par_iter()
        .map(|mv| {
            let mut probe = state.clone();
            // We re-validate inside `make_move`, which is fine.
            let _ = probe.make_move(mv);
            (mv, material(&probe, me))
        })
        .max_by_key(|&(_, score)| score)
        .map(|(mv, _)| mv)
}

/// Same logic as `best_move_parallel`, but sequential.  Useful as a
/// baseline for timing comparisons — the *only* difference is
/// `into_iter()` vs `into_par_iter()`, which is exactly the point of
/// the rayon abstraction.
pub fn best_move_sequential(state: &GameState) -> Option<Move> {
    let me = state.side_to_move;
    let moves = state.legal_moves();
    moves
        .into_iter()
        .map(|mv| {
            let mut probe = state.clone();
            let _ = probe.make_move(mv);
            (mv, material(&probe, me))
        })
        .max_by_key(|&(_, score)| score)
        .map(|(mv, _)| mv)
}
