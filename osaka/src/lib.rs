#![feature(const_option_ext)]

pub mod net;

pub mod park;

pub mod runtime;

pub mod sync;

pub mod time;

pub mod task;
pub use task::spawn;

/// Implementation detail of the `select!` macro. This macro is **not**
/// intended to be used as part of the public API and is permitted to
/// change.
#[doc(hidden)]
pub use osaka_derive::select_priv_declare_output_enum;

/// Implementation detail of the `select!` macro. This macro is **not**
/// intended to be used as part of the public API and is permitted to
/// change.
#[doc(hidden)]
pub use osaka_derive::select_priv_clean_pattern;

#[doc(inline)]
pub use osaka_derive::main_rt as main;

#[doc(inline)]
pub use osaka_derive::test_rt as test;

mod adapter;
mod macros;
mod util;
