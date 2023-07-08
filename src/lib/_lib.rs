//! # omake (Oxidized Make)
//!
//! This is the library component of `omake`, generally oriented towards the main binary of this
//! crate, but should be designed to be used by other applications.

mod context;
mod error;
mod expand;
mod logger;
mod makefile;
mod vars;

pub use context::Context;
pub use logger::{DefaultLogger, Logger, ERROR, INFO, WARN};
pub use makefile::{Makefile, Opts};
pub use vars::Env;
