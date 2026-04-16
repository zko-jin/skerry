//! # Skerry: Super Kool ERRors Yoh
//!
//! Example:
//! ```rust
//! use skerry::*;
//!
//! // 1. Define your error boundary
//! # struct ErrorFromLib;
//! #[skerry_mod]
//! pub mod errors {
//!     pub struct DatabaseErr;
//!     pub struct AuthErr;
//!     pub struct ValidationErr;
//!
//!     pub struct LibErr(ErrorFromLib);
//!     impl From<ErrorFromLib> for LibErr {
//!         fn from(val: ErrorFromLib) -> Self {
//!             Self(val)
//!         }
//!     }
//! }
//!
//! // 2. Generate a 'low_level' error enum automatically
//! #[skerry_fn]
//! fn check_auth() -> Result<(), AuthErr> {
//!     Err(CheckAuthError::AuthErr(AuthErr))
//! }
//!
//! # fn lib_fn_that_returns_error() -> Result<(), ErrorFromLib> {
//! #   Err(ErrorFromLib)
//! # }
//!
//! // 3. Use '&' to expand and bubble up sub-errors seamlessly
//! #[skerry_fn]
//! pub fn Controller() -> Result<(), (ValidationErr, LibErr, &CheckAuthError)> {
//!     // ValidationErr is local, AuthErr is pulled in from check_auth via '&'
//!     check_auth()?;
//!
//!     // You can also automatically bubble up library errors as long as an error from
//!     // `#[skerry_mod]` implements `From` for it
//!     lib_fn_that_returns_error()?;
//!
//!     Ok(())
//! }
//! ```
//!
//! Skerry is a type-safe error management framework designed to kill boilerplate.
//! It allows you to define a global error set while returning granular, function-specific
//! enums that are automatically generated at compile-time.
//!
//! ## Core Workflow
//!
//! 1. Define all possible error structs in a `#[skerry_mod]`.
//! 2. Mark functions with `#[skerry_fn]`.
//! 3. Use the `&` operator to bubble up errors from sub-functions without manually mapping variants.
//!
//! ---
//!
//! ## The Error Module
//! Every project needs one module (usually `errors.rs`) that acts as the source of truth.
//!
//! ```rust
//! pub use skerry::*; // Recommended to be pub for easier macro expansions
//!
//! #[skerry_mod]
//! mod errors {
//!     pub struct ErrA;
//!     pub struct ErrB;
//!     pub struct ErrC;
//!     pub struct DatabaseErr;
//! }
//! ```
//! *Note: When using errors in any other file, import them via `crate::errors::*;` instead
//! of individual imports to ensure the macros can resolve the paths correctly.*
//!
//! ---
//!
//! ## Function-Specific Enums
//!
//! By using `#[skerry_fn]`, you define a return type using a tuple of error structs.
//! Skerry transforms this into a unique enum named `{FunctionName}Error`.
//!
//! ```rust
//! # pub use skerry::*;
//! # #[skerry_mod]
//! # mod errors {
//! #     pub struct ErrA;
//! #     pub struct ErrB;
//! #     pub struct ErrC;
//! #     pub struct DatabaseErr;
//! # }
//! #[skerry_fn]
//! pub fn low_level() -> Result<(), (ErrA, ErrB)> {
//!     // Generates LowLevelError { ErrA(ErrA), ErrB(ErrB) }
//!     Err(LowLevelError::ErrA(ErrA)) // You can also type Err(ErrA.into())
//! }
//! ```
//!
//! ---
//!
//! ## The Ampersand (`&`) Expansion
//!
//! The `&` operator is the heart of Skerry. When you put `&OtherFnError` in your return tuple:
//!
//! * **Expansion**: It pulls all variants from `OtherFnError` into your current function's list.
//! * **Promotion**: It allows the `?` operator to work seamlessly for that function's return type.
//! * **Deduplication**: Variants are deduplicated automatically. If `ErrA` is added manually
//!   and also exists inside a `&` expansion, only one variant is generated.
//!
//! ```rust
//! # pub use skerry::*;
//! # #[skerry_mod]
//! # mod errors {
//! #     pub struct ErrA;
//! #     pub struct ErrB;
//! #     pub struct ErrC;
//! #     pub struct DatabaseErr;
//! # }
//! # #[skerry_fn]
//! # pub fn low_level() -> Result<(), (ErrA, ErrB)> {
//! #     // Generates LowLevelError { ErrA(ErrA), ErrB(ErrB) }
//! #     Err(LowLevelError::ErrA(ErrA)) // You can also type Err(ErrA.into())
//! # }
//! #[skerry_fn]
//! pub fn high_level() -> Result<(), (ErrC, &LowLevelError)> {
//!     // 1. Sees ErrC -> Adds variant
//!     // 2. Sees &LowLevelError -> Inspects LowLevelError, finds (ErrA, ErrB)
//!     // 3. Final HighLevelError contains variants: ErrA, ErrB, ErrC
//!
//!     low_level()?; // Bubbles up automatically
//!     Ok(())
//! }
//! ```
//!
//! The syntax below has the exact same effects, `&LowLevelError` is nothing more than syntatic sugar
//!
//! ```rust
//! # pub use skerry::*;
//! # #[skerry_mod]
//! # mod errors {
//! #     pub struct ErrA;
//! #     pub struct ErrB;
//! #     pub struct ErrC;
//! #     pub struct DatabaseErr;
//! # }
//! # #[skerry_fn]
//! # pub fn low_level() -> Result<(), (ErrA, ErrB)> {
//! #     // Generates LowLevelError { ErrA(ErrA), ErrB(ErrB) }
//! #     Err(LowLevelError::ErrA(ErrA)) // You can also type Err(ErrA.into())
//! # }
//! #[skerry_fn]
//! pub fn high_level() -> Result<(), (ErrA, ErrB, ErrC)> {
//!     // ...
//!     # Ok(())
//! }
//! ```
//!
//! In the cases above the generated enum looks like this
//! ```rust
//! # struct ErrA;
//! # struct ErrB;
//! # struct ErrC;
//! pub enum HighLevelError {
//!     ErrA(ErrA),
//!     ErrB(ErrB),
//!     ErrC(ErrC),
//! }
//! ```
//! ## Using Skerry inside Impl Blocks
//!
//! Skerry provides the `#[skerry_impl]` attribute to handle methods within `impl` blocks.
//! This attribute coordinates with `#[skerry_fn]` to split the generated code:
//!
//! 1.  **Top-Level**: The error enums are generated outside the `impl` block.
//! 2.  **Method-Level**: The method signature is updated, and all `?` operators are
//!     automatically transformed to wrap errors into `GlobalErrors`.
//!
//! ### Example
//!
//! ```rust
//! use skerry::*;
//!
//! # #[skerry_mod]
//! # mod errors {
//! #   pub struct ConnectionFailed;
//! # }
//! # #[skerry_fn]
//! # pub fn remote_call() -> Result<(), (ConnectionFailed)> {
//! #    Ok(())
//! # }
//! #
//! pub struct Database;
//!
//! #[skerry_impl(prefix(Database))] // Optional prefix for functions inside impl block
//! impl Database {
//!     #[skerry_fn]
//!     pub fn connect(&self) -> Result<(), (&RemoteCallError)> {
//!         remote_call()?;
//!         Ok(())
//!     }
//! }
//!
//! fn main() {
//!     let db = Database;
//!     let result: Result<(), DatabaseConnectError> = db.connect();
//!     assert!(result.is_ok());
//! }
//! ```
//!
//! ---
//!
//! ## Compile-Time Safety
//!
//! Skerry uses a custom trait system (`MissingConvert`) to verify error bounds at
//! compile-time. If you try to use `?` on a function whose errors are not represented
//! in your current return tuple, the compiler will refuse to build.
#![allow(unused_features)]
#![feature(try_trait_v2)]

mod helpers;
mod macros;
mod traits;
pub use skerry_macros::{skerry_fn, skerry_impl, skerry_mod};

pub mod skerry_internals {
    pub use crate::{helpers::*, macros::*, traits::*};
    pub use skerry_macros::*;
}

#[cfg(test)]
mod test {
    extern crate self as skerry;
    pub use skerry::*;

    pub struct OuterErrorFromLib;

    #[skerry_mod]
    pub mod errors {
        pub struct ErrA;
        pub struct ErrB;
        pub struct ErrC;
        pub struct ErrD;
        pub struct ErrE;
        pub struct ErrF;
        pub struct Outer(OuterErrorFromLib);

        impl From<OuterErrorFromLib> for Outer {
            fn from(val: OuterErrorFromLib) -> Self {
                Self(val)
            }
        }
    }

    #[skerry_fn]
    fn my_fn1() -> Result<(), (ErrA, ErrB, ErrC)> {
        Err(MyFn1Error::ErrA(ErrA))
    }

    #[skerry_fn]
    fn my_fn2() -> Result<(), (ErrE, ErrF, Outer)> {
        let r: Result<(), OuterErrorFromLib> = Err(OuterErrorFromLib);
        let _ = r?;

        Ok(())
    }

    #[skerry_fn]
    pub fn my_fn3() -> Result<(), (ErrA, ErrB, ErrC, &MyFn2Error)> {
        my_fn2()?;
        my_fn1()?;
        Ok(())
    }

    pub struct MyStruct;

    #[skerry_impl(prefix(MyStruct))]
    impl MyStruct {
        #[skerry_fn]
        pub fn struct_fn() -> Result<(), (&MyFn3Error)> {
            my_fn3()?;
            Ok(())
        }
    }

    #[test]
    pub fn test() {
        let _ = my_fn3();
    }
}
