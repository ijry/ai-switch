//! MCP configuration management.
//!
//! Derived from the MCP settings implementation in xintaofei/codeg
//! (Apache-2.0), then adapted to AI Switch's transport and error model.

mod clients;
mod model;
mod normalize;
mod service;

pub(crate) mod command;
pub(crate) mod marketplace;
