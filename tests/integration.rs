//! Integration tests — exercise the public API as a downstream user
//! would.

use chess::board::Board;
use chess::game::{GameState, GameStatus};
use chess::moves::{Move, MoveKind, Square};
use chess::piece::{Color, PieceKind};

/// perft helper — count leaves at `depth` plies.
fn perft(state: &GameState, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let moves = state.legal_moves();
    if depth == 1 {
        return moves.len() as u64;
    }
    let mut total = 0u64;
    for mv in moves {
        let mut next = state.clone();
        next.make_move(mv).unwrap();
        total += perft(&next, depth - 1);
    }
    total
}

/// The published perft values for the standard starting position.
/// If our move generator is correct, we *must* match these exactly.
#[test]
fn perft_matches_published_values_through_depth_3() {
    let state = GameState::new();
    assert_eq!(perft(&state, 1), 20);
    assert_eq!(perft(&state, 2), 400);
    assert_eq!(perft(&state, 3), 8_902);
}

/// Depth 4 (~200k nodes) takes ~200 ms in debug mode; we keep it
/// behind `--ignored` so the regular `cargo test` stays fast, and
/// run it in CI with `cargo test -- --ignored`.
#[test]
#[ignore]
fn perft_matches_published_values_at_depth_4() {
    let state = GameState::new();
    assert_eq!(perft(&state, 4), 197_281);
}

#[test]
fn fen_placement_round_trips() {
    let original = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";
    let board = Board::from_fen(original).unwrap();
    assert_eq!(board.to_fen_placement(), original);
}

#[test]
fn opening_pawn_pushes_legal() {
    let g = GameState::new();
    let e2 = Square::parse("e2").unwrap();
    let from_e2: Vec<Move> = g.legal_moves_from(e2);
    assert_eq!(from_e2.len(), 2, "e2 pawn has two legal pushes initially");
}

#[test]
fn promotion_produces_four_choices() {
    // White pawn on e7, kings on a-file/h-file so e8 is empty, white to move.
    let board = Board::from_fen("7k/4P3/8/8/8/8/8/K7").unwrap();
    let g = GameState {
        board,
        side_to_move: Color::White,
        castling: chess::board::CastlingRights::default(),
        en_passant: None,
        halfmove_clock: 0,
        fullmove_number: 1,
        history: Vec::new(),
        status: GameStatus::Ongoing,
    };
    let promos: Vec<Move> = g
        .legal_moves()
        .into_iter()
        .filter(|m| matches!(m.kind, MoveKind::Promotion { .. }))
        .collect();
    // 4 promotion choices for the e7-e8 push.
    assert_eq!(promos.len(), 4);
    let kinds: Vec<PieceKind> = promos
        .iter()
        .filter_map(|m| match m.kind {
            MoveKind::Promotion { promote_to, .. } => Some(promote_to),
            _ => None,
        })
        .collect();
    assert!(kinds.contains(&PieceKind::Queen));
    assert!(kinds.contains(&PieceKind::Knight));
}

#[test]
fn en_passant_capture_works() {
    // Setup: White pawn on e5, Black pawn just moved d7-d5.
    // FEN-ish placement, then we'll set en_passant manually.
    let board = Board::from_fen("4k3/8/8/3pP3/8/8/8/4K3").unwrap();
    let mut g = GameState {
        board,
        side_to_move: Color::White,
        castling: chess::board::CastlingRights::default(),
        en_passant: Some(Square::parse("d6").unwrap()),
        halfmove_clock: 0,
        fullmove_number: 1,
        history: Vec::new(),
        status: GameStatus::Ongoing,
    };
    let from = Square::parse("e5").unwrap();
    let to = Square::parse("d6").unwrap();
    let played = g
        .make_move(Move::new(from, to, MoveKind::EnPassant))
        .expect("ep capture should be legal");
    assert!(matches!(played.kind, MoveKind::EnPassant));
    // Captured pawn (d5) is gone.
    assert!(g.board.get(Square::parse("d5").unwrap()).is_none());
    // Capturing pawn now on d6.
    assert_eq!(
        g.board.get(Square::parse("d6").unwrap()).unwrap().kind,
        PieceKind::Pawn
    );
}
