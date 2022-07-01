#![allow(clippy::needless_doctest_main)]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

//! Macros for use with Osaka

// This `extern` is required for older `rustc` versions but newer `rustc`
// versions warn about the unused `extern crate`.
#[allow(unused_extern_crates)]
extern crate proc_macro;

mod entry;
mod select;

use proc_macro::TokenStream;

/// Marks async function to be executed by the selected runtime. This macro
/// helps set up a `Runtime` without requiring the user to use
/// [Runtime](../osaka/runtime/struct.Runtime.html) or
/// [Builder](../osaka/runtime/struct.Builder.html) directly.
///
/// Note: This macro is designed to be simplistic and targets applications that
/// do not require a complex setup. If the provided functionality is not
/// sufficient, you may be interested in using
/// [Builder](../osaka/runtime/struct.Builder.html), which provides a more
/// powerful interface.
///
/// Note: This macro can be used on any function and not just the `main`
/// function. Using it on a non-main function makes the function behave as if it
/// was synchronous by starting a new runtime each time it is called. If the
/// function is called often, it is preferable to create the runtime using the
/// runtime builder so the runtime can be reused across calls.
///
/// # Multi-threaded runtime
///
/// To use the multi-threaded runtime, the macro can be configured using
///
/// ```ignore
/// #[osaka::main(flavor = "multi_thread", worker_threads = 10)]
/// # async fn main() {}
/// ```
///
/// The `worker_threads` option configures the number of worker threads, and
/// defaults to the number of cpus on the system. This is the default flavor.
///
/// Note: The multi-threaded runtime requires the `rt-multi-thread` feature
/// flag.
///
/// # Current thread runtime
///
/// To use the single-threaded runtime known as the `current_thread` runtime,
/// the macro can be configured using
///
/// ```ignore
/// #[osaka::main(flavor = "current_thread")]
/// # async fn main() {}
/// ```
///
/// ## Function arguments:
///
/// Arguments are allowed for any functions aside from `main` which is special
///
/// ## Usage
///
/// ### Using the multi-thread runtime
///
/// ```ignore
/// #[osaka::main]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[osaka::main]`
///
/// ```ignore
/// fn main() {
///     osaka::runtime::Builder::new_multi_thread()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
///
/// ### Using current thread runtime
///
/// The basic scheduler is single-threaded.
///
/// ```ignore
/// #[osaka::main(flavor = "current_thread")]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[osaka::main]`
///
/// ```ignore
/// fn main() {
///     osaka::runtime::Builder::new_current_thread()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
///
/// ### Set number of worker threads
///
/// ```ignore
/// #[osaka::main(worker_threads = 2)]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[osaka::main]`
///
/// ```ignore
/// fn main() {
///     osaka::runtime::Builder::new_multi_thread()
///         .worker_threads(2)
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
///
/// ### Configure the runtime to start with time paused
///
/// ```ignore
/// #[osaka::main(flavor = "current_thread", start_paused = true)]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[osaka::main]`
///
/// ```ignore
/// fn main() {
///     osaka::runtime::Builder::new_current_thread()
///         .enable_all()
///         .start_paused(true)
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
///
/// Note that `start_paused` requires the `test-util` feature to be enabled.
///
/// ### Rename package
///
/// ```ignore
/// use osaka as osaka1;
///
/// #[osaka1::main(crate = "osaka1")]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[osaka::main]`
///
/// ```ignore
/// use osaka as osaka1;
///
/// fn main() {
///     osaka1::runtime::Builder::new_multi_thread()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
#[proc_macro_attribute]
#[cfg(not(test))] // Work around for rust-lang/rust#62127
pub fn main(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::main(args, item, true)
}

/// Marks async function to be executed by selected runtime. This macro helps set up a `Runtime`
/// without requiring the user to use [Runtime](../osaka/runtime/struct.Runtime.html) or
/// [Builder](../osaka/runtime/struct.builder.html) directly.
///
/// ## Function arguments:
///
/// Arguments are allowed for any functions aside from `main` which is special
///
/// ## Usage
///
/// ### Using default
///
/// ```ignore
/// #[osaka::main(flavor = "current_thread")]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[osaka::main]`
///
/// ```ignore
/// fn main() {
///     osaka::runtime::Builder::new_current_thread()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
///
/// ### Rename package
///
/// ```ignore
/// use osaka as osaka1;
///
/// #[osaka1::main(crate = "osaka1")]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[osaka::main]`
///
/// ```ignore
/// use osaka as osaka1;
///
/// fn main() {
///     osaka1::runtime::Builder::new_multi_thread()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
#[proc_macro_attribute]
#[cfg(not(test))] // Work around for rust-lang/rust#62127
pub fn main_rt(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::main(args, item, false)
}

/// Marks async function to be executed by runtime, suitable to test environment.
/// This macro helps set up a `Runtime` without requiring the user to use
/// [Runtime](../osaka/runtime/struct.Runtime.html) or
/// [Builder](../osaka/runtime/struct.Builder.html) directly.
///
/// Note: This macro is designed to be simplistic and targets applications that
/// do not require a complex setup. If the provided functionality is not
/// sufficient, you may be interested in using
/// [Builder](../osaka/runtime/struct.Builder.html), which provides a more
/// powerful interface.
///
/// # Multi-threaded runtime
///
/// To use the multi-threaded runtime, the macro can be configured using
///
/// ```ignore
/// #[osaka::test(flavor = "multi_thread", worker_threads = 1)]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
///
/// The `worker_threads` option configures the number of worker threads, and
/// defaults to the number of cpus on the system. This is the default
/// flavor.
///
/// Note: The multi-threaded runtime requires the `rt-multi-thread` feature
/// flag.
///
/// # Current thread runtime
///
/// The default test runtime is single-threaded. Each test gets a
/// separate current-thread runtime.
///
/// ```no_run
/// #[osaka::test]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
///
/// ## Usage
///
/// ### Using the multi-thread runtime
///
/// ```ignore
/// #[osaka::test(flavor = "multi_thread")]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
///
/// Equivalent code not using `#[osaka::test]`
///
/// ```no_run
/// #[test]
/// fn my_test() {
///     osaka::runtime::Builder::new_multi_thread()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             assert!(true);
///         })
/// }
/// ```
///
/// ### Using current thread runtime
///
/// ```no_run
/// #[osaka::test]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
///
/// Equivalent code not using `#[osaka::test]`
///
/// ```no_run
/// #[test]
/// fn my_test() {
///     osaka::runtime::Builder::new_current_thread()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             assert!(true);
///         })
/// }
/// ```
///
/// ### Set number of worker threads
///
/// ```ignore
/// #[osaka::test(flavor ="multi_thread", worker_threads = 2)]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
///
/// Equivalent code not using `#[osaka::test]`
///
/// ```no_run
/// #[test]
/// fn my_test() {
///     osaka::runtime::Builder::new_multi_thread()
///         .worker_threads(2)
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             assert!(true);
///         })
/// }
/// ```
///
/// ### Configure the runtime to start with time paused
///
/// ```no_run
/// #[osaka::test(start_paused = true)]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
///
/// Equivalent code not using `#[osaka::test]`
///
/// ```no_run
/// #[test]
/// fn my_test() {
///     osaka::runtime::Builder::new_current_thread()
///         .enable_all()
///         .start_paused(true)
///         .build()
///         .unwrap()
///         .block_on(async {
///             assert!(true);
///         })
/// }
/// ```
///
/// Note that `start_paused` requires the `test-util` feature to be enabled.
///
/// ### Rename package
///
/// ```rust
/// use osaka as osaka1;
///
/// #[osaka1::test(crate = "osaka1")]
/// async fn my_test() {
///     println!("Hello world");
/// }
/// ```
#[proc_macro_attribute]
pub fn test(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::test(args, item, true)
}

/// Marks async function to be executed by runtime, suitable to test environment
///
/// ## Usage
///
/// ```no_run
/// #[osaka::test]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
#[proc_macro_attribute]
pub fn test_rt(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::test(args, item, false)
}

/// Always fails with the error message below.
/// ```text
/// The #[osaka::main] macro requires rt or rt-multi-thread.
/// ```
#[proc_macro_attribute]
pub fn main_fail(_args: TokenStream, _item: TokenStream) -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "The #[osaka::main] macro requires rt or rt-multi-thread.",
    )
    .to_compile_error()
    .into()
}

/// Always fails with the error message below.
/// ```text
/// The #[osaka::test] macro requires rt or rt-multi-thread.
/// ```
#[proc_macro_attribute]
pub fn test_fail(_args: TokenStream, _item: TokenStream) -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "The #[osaka::test] macro requires rt or rt-multi-thread.",
    )
    .to_compile_error()
    .into()
}

/// Implementation detail of the `select!` macro. This macro is **not** intended
/// to be used as part of the public API and is permitted to change.
#[proc_macro]
#[doc(hidden)]
pub fn select_priv_declare_output_enum(input: TokenStream) -> TokenStream {
    select::declare_output_enum(input)
}

/// Implementation detail of the `select!` macro. This macro is **not** intended
/// to be used as part of the public API and is permitted to change.
#[proc_macro]
#[doc(hidden)]
pub fn select_priv_clean_pattern(input: TokenStream) -> TokenStream {
    select::clean_pattern_macro(input)
}