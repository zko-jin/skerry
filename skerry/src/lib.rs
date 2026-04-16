//! # Skerry: Super Kool ERRors Yoh
//!
//! Example:
//! ```rust
//! use skerry::*;
//!
//! // 1. Define your error boundary
//! #[error_module]
//! pub mod errors {
//!     pub struct DatabaseErr;
//!     pub struct AuthErr;
//!     pub struct ValidationErr;
//! }
//!
//! // 2. Generate a 'low_level' error enum automatically
//! #[fn_error]
//! fn check_auth() -> Result<(), AuthErr> {
//!     Err(CheckAuthError::AuthErr(AuthErr))
//! }
//!
//! // 3. Use '&' to expand and bubble up sub-errors seamlessly
//! #[fn_error]
//! pub fn Controller() -> Result<(), (ValidationErr, &CheckAuthError)> {
//!     // ValidationErr is local, AuthErr is pulled in from check_auth via '&'
//!     check_auth()?;
//!     Ok(())
//! }
//! ```
//!
//! Known Issues:
//! - There's no macro for handling `impl` blocks
//! - Same for `trait` blocks
//!
//! Skerry is a type-safe error management framework designed to kill boilerplate.
//! It allows you to define a global error set while returning granular, function-specific
//! enums that are automatically generated at compile-time.
//!
//! ## Core Workflow
//!
//! 1. **Centralize**: Define all possible error structs in a `#[error_module]`.
//! 2. **Annotate**: Mark functions with `#[fn_error]`.
//! 3. **Compose**: Use the `&` operator to bubble up errors from sub-functions without manually mapping variants.
//!
//! ---
//!
//! ## The Error Module
//! Every project needs one module (usually `errors.rs`) that acts as the source of truth.
//!
//! ```rust,ignore
//! pub use skerry::*; // Required to be pub for macro expansion
//!
//! #[error_module]
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
//! By using `#[fn_error]`, you define a return type using a tuple of error structs.
//! Skerry transforms this into a unique enum named `{FunctionName}Error`.
//!
//! ```rust
//! #[fn_error]
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
//! ```rust,ignore
//! #[fn_error]
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
//! ```rust,ignore
//! #[fn_error]
//! pub fn high_level() -> Result<(), (ErrA, ErrB, ErrC)> {
//!     // ...
//! }
//! ```
//!
//! In the cases above the generated enum looks like this
//! ```rust,ignore
//! pub enum HighLevelError {
//!     ErrA(ErrA),
//!     ErrB(ErrB),
//!     ErrC(ErrC),
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
mod result;
mod traits;

pub use skerry_macros::{error_module, fn_error};

pub mod skerry_internals {
    pub use crate::{helpers::*, macros::*, traits::*};
    pub use skerry_macros::*;
}

#[cfg(test)]
mod test {
    extern crate self as skerry;
    pub use skerry::*;

    #[error_module]
    pub mod errors {
        pub struct ErrA;
        pub struct ErrB;
        pub struct ErrC;
        pub struct ErrD;
        pub struct ErrE;
        pub struct ErrF;
        pub struct ErrG;
    }

    #[fn_error]
    fn my_fn1() -> Result<(), (ErrA, ErrB, ErrC)> {
        Err(MyFn1Error::ErrA(ErrA))
    }

    #[fn_error]
    fn my_fn2() -> Result<(), (ErrE, ErrF, ErrG)> {
        Err(MyFn2Error::ErrE(ErrE))
    }

    #[fn_error]
    pub fn my_fn3() -> Result<(), (ErrA, ErrB, ErrC, &MyFn2Error)> {
        my_fn2()?;
        my_fn1()?;
        Ok(())
    }

    #[test]
    pub fn test() {
        my_fn3();
    }
}
