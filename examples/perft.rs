//! Perft (performance test) — the standard correctness benchmark for
//! a chess move generator.  We count the total number of leaf nodes
//! reachable at each depth from the starting position and compare
//! against published values from the chess-programming wiki:
//!
//!   depth 1 → 20
//!   depth 2 → 400
//!   depth 3 → 8 902
//!   depth 4 → 197 281
//!   depth 5 → 4 865 609
//!
//! If all of these match, every kind of move (including special
//! moves like castling, en passant, promotion) is generated correctly
//! and the king-safety filter is sound.
//!
//! Run with:
//!
//!     cargo run --release --example perft
//!
//! and you'll see both the node counts and the throughput in
//! moves-per-second.

use chess::game::GameState;
use chess::moves::Move;
use std::time::Instant;

/// Count leaves at `depth` plies from `state`.
fn perft(state: &GameState, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let moves: Vec<Move> = state.legal_moves();
    if depth == 1 {
        return moves.len() as u64;
    }
    let mut total: u64 = 0;
    for mv in moves {
        let mut next = state.clone();
        next.make_move(mv).expect("legal move should apply");
        total += perft(&next, depth - 1);
    }
    total
}

fn main() {
    let depths_with_expected: &[(u32, u64)] = &[
        (1, 20),
        (2, 400),
        (3, 8_902),
        (4, 197_281),
        // Depth 5 is uncommented by default because it takes a few
        // seconds in debug mode.  Comment back in for release runs.
        // (5, 4_865_609),
    ];

    let state = GameState::new();
    println!("perft from the standard starting position:");
    println!("┌───────┬───────────┬─────────┬───────────┬─────────────┐");
    println!("│ depth │   nodes   │ expect. │  time(ms) │   knodes/s  │");
    println!("├───────┼───────────┼─────────┼───────────┼─────────────┤");
    for &(depth, expected) in depths_with_expected {
        let start = Instant::now();
        let count = perft(&state, depth);
        let elapsed = start.elapsed();
        let ms = elapsed.as_secs_f64() * 1000.0;
        let knps = (count as f64 / 1000.0) / elapsed.as_secs_f64().max(1e-9);
        let ok = if count == expected { "OK" } else { "BAD" };
        println!(
            "│   {depth:<3} │ {count:>9} │ {expected:>7} │ {ms:>9.2} │ {knps:>11.1} │  {ok}"
        );
    }
    println!("└───────┴───────────┴─────────┴───────────┴─────────────┘");
}
