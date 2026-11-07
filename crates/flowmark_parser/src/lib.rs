//! Parser for Flowmark review files (`*.review.ts`). See `SPEC.md` at the repository root.
//!
//! ```ignore
//! let doc = flowmark_parser::parse(&std::fs::read_to_string("register-user.review.ts").unwrap());
//! for d in &doc.diagnostics {
//!     eprintln!("{}: {} {}", d.line, d.code, d.message);
//! }
//! ```
//!
//! The TypeScript syntax comes from [tsrs](https://github.com/oxc-project/tsrs). tsrs allocates syntax trees
//! in an arena that is never freed, so every call to [`parse`] keeps its tree in memory. That is fine for a
//! CLI; a long-running process should parse in a worker it can restart.

mod card;
mod grammar;
pub mod model;
mod parse;
mod text;
pub mod view;
pub mod viewmodel;

pub use grammar::{canonical_symbol, parse_comment, parse_target};
pub use model::*;
pub use parse::{SECTIONS, parse, parse_trigger};
