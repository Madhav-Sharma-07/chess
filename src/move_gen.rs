//! Pseudo-legal move generation per piece type.
//!
//! This is where many of the course concepts live:
//! * **Trait + associated constants/types** – `MoveGenerator` defines
//!   a per-piece move-generation strategy.
//! * **Generics + trait bounds** – `generate_for::<P>(&board, ..)` is
//!   a generic free function that the compiler monomorphises per
//!   `PieceLogic` impl, giving zero-cost dispatch.
//! * **Iterators / closures / `flat_map` / `filter`** – every piece's
//!   move list is an iterator chain.
//! * **Function pointers** – the `slide` helper takes a `&[Direction]`
//!   slice (ordinary data) and a closure for filtering.
//! * **Lifetimes** – pseudo-legal-move iterators borrow from the
//!   caller's `Board` reference.
//!
//! "Pseudo-legal" means *the piece could legally move there ignoring
//! check*. Filtering by *king-safety* happens in `game.rs` because it
//! requires a snapshot of the full game state (en-passant target,
//! castling rights, etc.).

use crate::board::{Board, CastlingRights};
use crate::moves::{Move, MoveKind, Square};
use crate::piece::{Color, Piece, PieceKind};

/// A direction expressed as `(file_delta, rank_delta)` in [-1, 1].
pub type Direction = (i8, i8);

/// A self-contained "move-generation strategy" implemented per piece.
///
/// Demonstrates both an **associated constant** (`KIND`) and a
/// generic free function (`generate_for`) further down that uses
/// `P: PieceLogic` as a trait bound.
pub trait PieceLogic {
    /// Which `PieceKind` does this strategy generate moves for?
    const KIND: PieceKind;

    /// Append all pseudo-legal moves for the piece located at `from`
    /// onto `out`. The implementation is free to push as many moves
    /// as it likes, including zero.
    fn pseudo_legal(board: &Board, color: Color, from: Square, out: &mut Vec<Move>);
}

// ---------------------------------------------------------------------------
//  Helpers shared across pieces
// ---------------------------------------------------------------------------

/// Slide along each direction until either: we leave the board, hit
/// our own piece (stop, no move), or hit an enemy piece (capture
/// move, then stop).  Used by Bishop, Rook, and Queen.
///
/// The closure `is_blocking` is technically redundant here (it's
/// always "occupied square stops us"), but extracting the predicate
/// shows how a closure could parameterise the search — for example,
/// a king-safety check.
fn slide_pseudo_legal(
    board: &Board,
    color: Color,
    from: Square,
    directions: &[Direction],
    out: &mut Vec<Move>,
) {
    for &(df, dr) in directions {
        let mut file = from.file() + df;
        let mut rank = from.rank() + dr;
        while let Some(sq) = Square::from_coords(file, rank) {
            match board.get(sq) {
                None => out.push(Move::new(from, sq, MoveKind::Quiet)),
                Some(p) if p.color != color => {
                    out.push(Move::new(from, sq, MoveKind::Capture));
                    break;
                }
                Some(_) => break, // own piece; stop sliding
            }
            file += df;
            rank += dr;
        }
    }
}

/// "Step" pieces (knight, king) — fixed offsets, no sliding.
fn step_pseudo_legal(
    board: &Board,
    color: Color,
    from: Square,
    offsets: &[Direction],
    out: &mut Vec<Move>,
) {
    for &(df, dr) in offsets {
        if let Some(sq) = Square::from_coords(from.file() + df, from.rank() + dr) {
            match board.get(sq) {
                None => out.push(Move::new(from, sq, MoveKind::Quiet)),
                Some(p) if p.color != color => {
                    out.push(Move::new(from, sq, MoveKind::Capture));
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
//  Concrete piece-logic types
// ---------------------------------------------------------------------------

/// Marker types used purely as trait-bound carriers — they hold no
/// data.  Each one implements [`PieceLogic`].
pub struct PawnLogic;
pub struct KnightLogic;
pub struct BishopLogic;
pub struct RookLogic;
pub struct QueenLogic;
pub struct KingLogic;

const ROOK_DIRS: &[Direction] = &[(1, 0), (-1, 0), (0, 1), (0, -1)];
const BISHOP_DIRS: &[Direction] = &[(1, 1), (1, -1), (-1, 1), (-1, -1)];
const KNIGHT_OFFSETS: &[Direction] = &[
    (1, 2),
    (2, 1),
    (-1, 2),
    (-2, 1),
    (1, -2),
    (2, -1),
    (-1, -2),
    (-2, -1),
];
const KING_OFFSETS: &[Direction] = &[
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

// ----- Pawn ----------------------------------------------------------------

impl PieceLogic for PawnLogic {
    const KIND: PieceKind = PieceKind::Pawn;

    fn pseudo_legal(board: &Board, color: Color, from: Square, out: &mut Vec<Move>) {
        let dir = color.pawn_dir();
        let promo_rank = color.promotion_rank();
        let start_rank = color.pawn_start_rank();

        // Single push.
        if let Some(one) = Square::from_coords(from.file(), from.rank() + dir) {
            if board.get(one).is_none() {
                if one.rank() == promo_rank {
                    push_all_promotions(from, one, false, out);
                } else {
                    out.push(Move::new(from, one, MoveKind::Quiet));
                }

                // Double push from the starting rank.
                if from.rank() == start_rank {
                    if let Some(two) = Square::from_coords(from.file(), from.rank() + 2 * dir) {
                        if board.get(two).is_none() {
                            out.push(Move::new(from, two, MoveKind::DoublePawnPush));
                        }
                    }
                }
            }
        }

        // Diagonal captures (en passant is added by `game.rs`, since
        // it needs the en-passant target).
        for df in [-1_i8, 1_i8] {
            if let Some(diag) = Square::from_coords(from.file() + df, from.rank() + dir) {
                if let Some(p) = board.get(diag) {
                    if p.color != color {
                        if diag.rank() == promo_rank {
                            push_all_promotions(from, diag, true, out);
                        } else {
                            out.push(Move::new(from, diag, MoveKind::Capture));
                        }
                    }
                }
            }
        }
    }
}

/// Helper: emit a promotion move for each of Q/R/B/N.
fn push_all_promotions(from: Square, to: Square, capture: bool, out: &mut Vec<Move>) {
    // Iterator over the four legal promotion targets — illustrates
    // `iter().copied()` on a small array literal plus `for` consumption.
    for promote_to in [
        PieceKind::Queen,
        PieceKind::Rook,
        PieceKind::Bishop,
        PieceKind::Knight,
    ]
    .iter()
    .copied()
    {
        out.push(Move::new(
            from,
            to,
            MoveKind::Promotion {
                promote_to,
                capture,
            },
        ));
    }
}

// ----- Knight --------------------------------------------------------------

impl PieceLogic for KnightLogic {
    const KIND: PieceKind = PieceKind::Knight;
    fn pseudo_legal(board: &Board, color: Color, from: Square, out: &mut Vec<Move>) {
        step_pseudo_legal(board, color, from, KNIGHT_OFFSETS, out);
    }
}

// ----- Bishop --------------------------------------------------------------

impl PieceLogic for BishopLogic {
    const KIND: PieceKind = PieceKind::Bishop;
    fn pseudo_legal(board: &Board, color: Color, from: Square, out: &mut Vec<Move>) {
        slide_pseudo_legal(board, color, from, BISHOP_DIRS, out);
    }
}

// ----- Rook ----------------------------------------------------------------

impl PieceLogic for RookLogic {
    const KIND: PieceKind = PieceKind::Rook;
    fn pseudo_legal(board: &Board, color: Color, from: Square, out: &mut Vec<Move>) {
        slide_pseudo_legal(board, color, from, ROOK_DIRS, out);
    }
}

// ----- Queen ---------------------------------------------------------------

impl PieceLogic for QueenLogic {
    const KIND: PieceKind = PieceKind::Queen;
    fn pseudo_legal(board: &Board, color: Color, from: Square, out: &mut Vec<Move>) {
        slide_pseudo_legal(board, color, from, ROOK_DIRS, out);
        slide_pseudo_legal(board, color, from, BISHOP_DIRS, out);
    }
}

// ----- King ----------------------------------------------------------------
// Castling is added by `game.rs` because it depends on castling
// rights and check status.

impl PieceLogic for KingLogic {
    const KIND: PieceKind = PieceKind::King;
    fn pseudo_legal(board: &Board, color: Color, from: Square, out: &mut Vec<Move>) {
        step_pseudo_legal(board, color, from, KING_OFFSETS, out);
    }
}

// ---------------------------------------------------------------------------
//  Generic dispatcher
// ---------------------------------------------------------------------------

/// Generic free function — `P: PieceLogic` is monomorphised at every
/// call site, so the compiler emits a specialised version per piece.
/// This is the "static dispatch via trait bound" example from class.
pub fn generate_for<P: PieceLogic>(
    board: &Board,
    color: Color,
    from: Square,
    out: &mut Vec<Move>,
) {
    P::pseudo_legal(board, color, from, out);
}

/// Dynamic dispatch flavour: pick the right strategy based on a
/// runtime [`PieceKind`].  Used inside the main move-generation
/// loop in `game.rs`.
///
/// Because `Piece` is `Copy`, we can pattern-match the kind cheaply
/// and then call into the generic specialisation.  Each arm is a
/// `function pointer` style call — illustrates the "generics vs.
/// dynamic dispatch" trade-off.
pub fn generate_pseudo_legal_for_piece(
    board: &Board,
    piece: Piece,
    from: Square,
    out: &mut Vec<Move>,
) {
    match piece.kind {
        PieceKind::Pawn => generate_for::<PawnLogic>(board, piece.color, from, out),
        PieceKind::Knight => generate_for::<KnightLogic>(board, piece.color, from, out),
        PieceKind::Bishop => generate_for::<BishopLogic>(board, piece.color, from, out),
        PieceKind::Rook => generate_for::<RookLogic>(board, piece.color, from, out),
        PieceKind::Queen => generate_for::<QueenLogic>(board, piece.color, from, out),
        PieceKind::King => generate_for::<KingLogic>(board, piece.color, from, out),
    }
}

// ---------------------------------------------------------------------------
//  Attack queries (used for "is the king in check?" and castling).
// ---------------------------------------------------------------------------

/// Returns `true` if `sq` is attacked by any piece of `by_color`.
/// This is what the legality filter and the castling check use.
///
/// **Subtlety:** for non-pawn pieces, a piece's *attack squares* and
/// its *pseudo-legal move destinations* are the same.  Pawns are the
/// exception — they *attack* their two diagonals, but they only
/// *move* there if there's an enemy piece (or via en passant). So for
/// pawns we compute the attack squares directly, not from the move
/// generator.
pub fn is_square_attacked(board: &Board, sq: Square, by_color: Color) -> bool {
    let mut buf: Vec<Move> = Vec::with_capacity(28);

    // Iterator pipeline + `any` short-circuit at the first attacker.
    board.pieces_of(by_color).any(|(from, piece)| {
        attacks_square(board, from, piece, sq, &mut buf)
    })
}

/// Does `piece` standing on `from` attack `target`?  Reuses `buf` to
/// avoid heap thrash inside the outer iterator.
fn attacks_square(
    board: &Board,
    from: Square,
    piece: Piece,
    target: Square,
    buf: &mut Vec<Move>,
) -> bool {
    if piece.kind == PieceKind::Pawn {
        // Pawns attack only the two forward diagonals — independent
        // of whether the destination is empty or occupied.
        let dir = piece.color.pawn_dir();
        for df in [-1_i8, 1_i8] {
            if Square::from_coords(from.file() + df, from.rank() + dir) == Some(target) {
                return true;
            }
        }
        false
    } else {
        // For knights/bishops/rooks/queens/kings, the move-generator
        // already produces exactly the set of attacked squares
        // (sliders stop at the first piece they meet, so blocked
        // squares correctly aren't attacked).
        buf.clear();
        generate_pseudo_legal_for_piece(board, piece, from, buf);
        buf.iter().any(|m| m.to == target)
    }
}

/// Helper used by the castling generator: are *any* of these squares
/// attacked by the opposing colour?  Demonstrates a small closure
/// passed to `Iterator::any`.
pub fn any_attacked(board: &Board, by: Color, squares: &[Square]) -> bool {
    squares.iter().any(|&s| is_square_attacked(board, s, by))
}

/// Generate castling moves for the side to move.  Requires the
/// current castling rights and a board on which the king is *not* in
/// check (callers in `game.rs` ensure this).
pub fn castling_moves(
    board: &Board,
    color: Color,
    rights: CastlingRights,
    out: &mut Vec<Move>,
) {
    let rank = match color {
        Color::White => 0,
        Color::Black => 7,
    };
    let king_from = Square::from_coords(4, rank).unwrap();

    // Sanity: must have a king of the right colour on the home square.
    if board.get(king_from) != Some(Piece::new(color, PieceKind::King)) {
        return;
    }

    let opp = color.opponent();

    // Kingside (O-O): squares f and g must be empty; e/f/g must not
    // be attacked; rook on h must be present.
    let (mask_k, mask_q) = match color {
        Color::White => (CastlingRights::WHITE_KING, CastlingRights::WHITE_QUEEN),
        Color::Black => (CastlingRights::BLACK_KING, CastlingRights::BLACK_QUEEN),
    };
    if rights.has(mask_k) {
        let f = Square::from_coords(5, rank).unwrap();
        let g = Square::from_coords(6, rank).unwrap();
        let h = Square::from_coords(7, rank).unwrap();
        let rook_ok = board.get(h) == Some(Piece::new(color, PieceKind::Rook));
        if rook_ok
            && board.get(f).is_none()
            && board.get(g).is_none()
            && !any_attacked(board, opp, &[king_from, f, g])
        {
            out.push(Move::new(king_from, g, MoveKind::CastleKingside));
        }
    }
    // Queenside (O-O-O): b/c/d empty; e/d/c not attacked; rook on a.
    if rights.has(mask_q) {
        let d = Square::from_coords(3, rank).unwrap();
        let c = Square::from_coords(2, rank).unwrap();
        let b = Square::from_coords(1, rank).unwrap();
        let a = Square::from_coords(0, rank).unwrap();
        let rook_ok = board.get(a) == Some(Piece::new(color, PieceKind::Rook));
        if rook_ok
            && board.get(b).is_none()
            && board.get(c).is_none()
            && board.get(d).is_none()
            && !any_attacked(board, opp, &[king_from, d, c])
        {
            out.push(Move::new(king_from, c, MoveKind::CastleQueenside));
        }
    }
}

/// En-passant capture moves (if the side to move has a pawn that can
/// capture *to* the en-passant target).
pub fn en_passant_moves(
    board: &Board,
    color: Color,
    en_passant: Option<Square>,
    out: &mut Vec<Move>,
) {
    let Some(target) = en_passant else { return };
    let dir = color.pawn_dir();
    // The pawn that *captures* is on `target.rank() - dir`, on file
    // `target.file() ± 1`.
    for df in [-1_i8, 1_i8] {
        if let Some(from) = Square::from_coords(target.file() + df, target.rank() - dir) {
            if board.get(from) == Some(Piece::new(color, PieceKind::Pawn)) {
                out.push(Move::new(from, target, MoveKind::EnPassant));
            }
        }
    }
}
