//! Binary entry point.  All real logic lives in `lib.rs` and friends.
//!
//! `#[tokio::main]` macro turns `main` into an async function backed
//! by a multi-threaded runtime — that's the "Async Rust" topic from
//! the syllabus.

use anyhow::Result;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    chess::app::run().await
}
