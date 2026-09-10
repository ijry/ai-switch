mod adapters;
mod app_state;
#[cfg(feature = "desktop")]
mod commands;
mod config_writer;
mod core;
mod database;
mod error;
mod imagegen;
mod importers;
mod mcp;
mod models;
mod paths;
mod saas;
mod security;
pub mod server;
mod services;
mod session_manager;
mod skills;
mod terminal_manager;
mod web;

#[cfg(feature = "desktop")]
include!("desktop.rs");

// rfd uses TaskDialogIndirect, which requires the Common Controls v6 manifest.
// Tauri links its generated resource into application binaries, but not lib tests.
#[cfg(all(test, target_os = "windows", feature = "desktop"))]
#[link(name = "resource", kind = "static")]
unsafe extern "C" {}
