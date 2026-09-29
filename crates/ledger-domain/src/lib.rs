//! The shapes a ledger is made of, and the rules for cleaning input into them.
//! No I/O and no policy: the writer decides what may change, this says what a
//! record is.

pub mod document;
pub mod lineage;
pub mod money;
pub mod records;
pub mod text;

pub use document::{Ledger, SCHEMA_VERSION};
pub use lineage::{Lineage, Relation};
pub use money::Money;
pub use records::*;
pub use text::{new_id, plain, valid_id};
