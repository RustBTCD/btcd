//! Storage engines.
//!
//! Every engine lives here and implements [`Database`](super::db::Database). A node opens as
//! many as it likes, each with the tables it should hold, and decides in its own wiring which
//! table goes where.

pub mod rocks;

pub use rocks::{Rocks, RocksOptions};
