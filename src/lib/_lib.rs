//! # omake
//!
//! This is the library component of the project, responsible for parsing and executing makefiles.

pub mod error;
pub mod expand;
pub mod logger;
pub mod makefile;
pub mod vars;

pub use error::MakeError;
pub use logger::{DefaultLogger, Logger, context::Context};
pub use makefile::{Makefile, opts::Opts};
pub use vars::{Env, Vars};
