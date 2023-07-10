//! # omake (Oxidized Make)
//!
//! This is the library component of the project, responsible for parsing and executing makefiles.

pub mod context;
pub mod error;
pub mod expand;
pub mod logger;
pub mod makefile;
pub mod vars;

pub use context::Context;
pub use error::MakeError;
pub use logger::{DefaultLogger, Logger};
pub use makefile::opts::Opts;
pub use makefile::Makefile;
pub use vars::{Env, Vars};
