//! `chess` — CIS 1905 final project.
//!
//! Modules are kept narrow and well-commented; each one focuses on a
//! handful of language concepts so the presentation can step through
//! them in order:
//!
//! | Module          | Lecture topics demonstrated                           |
//! |-----------------|-------------------------------------------------------|
//! | `piece`         | enums, pattern matching, `Copy` types                 |
//! | `moves`         | enums with associated data, struct literals           |
//! | `board`         | const generics, lifetimes, iterators                  |
//! | `move_gen`      | trait + generics + trait bounds, function pointers    |
//! | `game`          | smart pointers (`Box`), iterator pipelines, closures  |
//! | `ui`            | `Cow<'static, str>`, fat pointers, lifetimes          |
//! | `app`           | async/await, `Arc<Mutex<>>`, mpsc channels, `Send`    |
//! | `ai` (feature)  | rayon parallel iterators, `Send`/`Sync` bounds        |

pub mod app;
pub mod board;
pub mod game;
pub mod move_gen;
pub mod moves;
pub mod piece;
pub mod ui;

#[cfg(feature = "ai")]
pub mod ai;
