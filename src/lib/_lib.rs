//! # omake (Oxidized Make)
//!
//! This is the library component of the `omake` project, responsible for parsing and executing
//! makefiles.

pub mod context;
pub mod error;
pub mod expand;
pub mod logger;
pub mod makefile;
pub mod vars;
