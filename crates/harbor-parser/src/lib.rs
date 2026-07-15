//! Streaming VT/ANSI parser core — zero dependencies.
//!
//! The parser is a pure byte state machine that emits recognized control
//! sequences into a [`Perform`] sink. It knows nothing about terminal screens,
//! rendering, or I/O.
//!
//! # Example
//!
//! ```ignore
//! use harbor_parser::{Parser, Perform, Params};
//!
//! struct MyHandler;
//! impl Perform for MyHandler {
//!     fn print(&mut self, ch: char) { /* … */ }
//!     fn execute(&mut self, byte: u8) { /* … */ }
//!     // … remaining methods
//!     # fn csi_dispatch(&mut self, _: &Params, _: &[u8], _: Option<u8>, _: u8) {}
//!     # fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}
//!     # fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}
//!     # fn hook(&mut self, _: &Params, _: &[u8], _: bool, _: u8) {}
//!     # fn put(&mut self, _: u8) {}
//!     # fn unhook(&mut self) {}
//!     # fn start_string(&mut self, _: u8) {}
//! }
//!
//! let mut parser = Parser::default();
//! let mut handler = MyHandler;
//! for byte in b"\x1b[31mhello\x1b[0m" {
//!     parser.advance(&mut handler, *byte);
//! }
//! ```

pub mod core;
pub mod params;
pub mod perform;

pub use core::Parser;
pub use params::{CsiAccumulator, Param, Params, Utf8State};
pub use perform::Perform;
