//! A small, *self-contained* `unsafe` demonstration.
//!
//! The unsafe topic from class is about **raw pointers**, **undefined
//! behaviour**, **FFI**, and using **Miri** to detect UB.  This module
//! shows the *most useful* of those four — raw pointers — used to
//! borrow two **disjoint** mutable references into the same array.
//!
//! Why is this `unsafe`?  The borrow checker only understands "all of
//! `&mut self`" or "none of it"; it can't see that two separate
//! indices into a slice are disjoint.  We use `*mut T` to bypass
//! that, but we *manually verify* the precondition `i != j` so the
//! aliasing rule is preserved.  If the precondition were violated
//! (`i == j`), we'd produce two `&mut` references to the same place,
//! which is **immediate UB** that Miri would flag.
//!
//! Run `cargo +nightly miri test --package chess` to verify.

use crate::piece::Piece;

/// Swap two squares of an `Option<Piece>` slice in place.
///
/// # Panics
/// Panics if `i == j` *or* if either index is out of bounds.  We
/// panic instead of `unsafe`-ly trusting the caller because the
/// performance gain isn't worth a footgun in user code.
///
/// # Why not just call `slice::swap`?
/// You should!  This function exists purely to demonstrate the
/// *shape* of an `unsafe` block with explicit safety reasoning.
pub fn swap_squares_unsafe(cells: &mut [Option<Piece>], i: usize, j: usize) {
    assert!(i != j, "indices must be distinct");
    assert!(i < cells.len() && j < cells.len(), "index out of bounds");

    // SAFETY:
    //  * `i != j` (asserted above), so the two pointers refer to
    //    *different* objects, satisfying Rust's no-aliasing rule for
    //    `&mut`.
    //  * Both indices are in bounds (asserted above), so the
    //    pointers are derived from valid allocations of `cells`.
    //  * `Option<Piece>` is `Copy`-ish (literally `Copy` here), so
    //    `ptr::swap` (which uses `read`/`write`) is fine.
    unsafe {
        let p_i: *mut Option<Piece> = cells.as_mut_ptr().add(i);
        let p_j: *mut Option<Piece> = cells.as_mut_ptr().add(j);
        std::ptr::swap(p_i, p_j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piece::{Color, PieceKind};

    #[test]
    fn swap_works() {
        let mut cells: Vec<Option<Piece>> = vec![None; 4];
        cells[0] = Some(Piece::new(Color::White, PieceKind::Pawn));
        cells[3] = Some(Piece::new(Color::Black, PieceKind::Knight));
        swap_squares_unsafe(&mut cells, 0, 3);
        assert_eq!(cells[0].unwrap().kind, PieceKind::Knight);
        assert_eq!(cells[3].unwrap().kind, PieceKind::Pawn);
    }

    #[test]
    #[should_panic(expected = "indices must be distinct")]
    fn same_index_panics() {
        let mut cells: Vec<Option<Piece>> = vec![None; 2];
        // Calling with i == j would be UB if we didn't assert.
        swap_squares_unsafe(&mut cells, 1, 1);
    }
}
