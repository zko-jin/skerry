//! # Skerry: Super Kool ERRors Yoh
//!
//! Example:
//! ```rust
//! use skerry::*;
//!
//! // Define your error boundary, there can be only one #[sherry_mod] in your project
//! # struct ErrorFromLib;
//! #[skerry_mod]
//! pub mod errors {
//!     pub struct DatabaseErr;
//!     pub struct AuthErr;
//!     pub struct ValidationErr;
//!
//!     pub struct InvalidParse;
//!
//!     pub struct LibErr(ErrorFromLib);
//!     impl From<ErrorFromLib> for LibErr {
//!         fn from(val: ErrorFromLib) -> Self {
//!             Self(val)
//!         }
//!     }
//! }
//!
//! // Generates a CheckAuthError enum automatically
//! #[skerry_fn]
//! fn check_auth() -> Result<(), e![AuthErr]> {
//!     Err(CheckAuthError::AuthErr(AuthErr))
//! }
//!
//! # fn lib_fn_that_returns_error() -> Result<(), ErrorFromLib> {
//! #   Err(ErrorFromLib)
//! # }
//!
//! struct Controller;
//!
//! #[skerry_impl(prefix(Controller))] // This allows #[skerry_fn] to run on impl blocks
//! impl Controller {
//!     // Use '*' to expand and bubble up sub-errors seamlessly.
//!     #[skerry_fn]
//!     pub fn run() -> Result<(), e![ValidationErr, LibErr, *CheckAuthError]> {
//!         // AuthErr is pulled in from check_auth via '*CheckAuthError'.
//!         check_auth()?;
//!
//!         // Automatically bubble up library errors as long as an error
//!         // from `#[skerry_mod]` implements `From` for it.
//!         lib_fn_that_returns_error()?;
//!
//!         Ok(())
//!     }
//! }
//! #[skerry_trait]
//! trait ToJson {
//!     #[skerry_fn]
//!     fn to_json(&self) -> Result<(), e![InvalidParse]>;
//! }
//!
//! #[skerry_impl]
//! impl ToJson for Controller {
//!     // Whenever you do not want to generate a new error just don't use e![]
//!     // this will instead reuse an existing error
//!     #[skerry_fn]
//!     fn to_json(&self) -> Result<(), ToJsonError> {
//!         Ok(())
//!     }
//! }
//! ```
//!
//! ## Core Workflow
//!
//! - Define all possible error structs in a `#[skerry_mod]`.
//! - Mark functions with `#[skerry_fn]`.
//! - Use the `*` operator to bubble up errors from sub-functions without manually mapping variants.
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
//! pub fn low_level() -> Result<(), e![ErrA, ErrB]> {
//!     // Generates LowLevelError { ErrA(ErrA), ErrB(ErrB) }
//!     Err(LowLevelError::ErrA(ErrA)) // You can also type Err(ErrA.into())
//! }
//! ```
//!
//! ---
//!
//! ## The Asterisk (`*`) Expansion
//!
//! When you put `*OtherFnError` in your return array it pulls all
//! variants from `OtherFnError` into your current function's list.
//!
//! * **Deduplication**: Variants are deduplicated automatically. If `ErrA` is added manually
//!   and also exists inside a `*` expansion, only one variant is generated.
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
//! # pub fn low_level() -> Result<(), e![ErrA, ErrB]> {
//! #     // Generates LowLevelError { ErrA(ErrA), ErrB(ErrB) }
//! #     Err(LowLevelError::ErrA(ErrA)) // You can also type Err(ErrA.into())
//! # }
//! #[skerry_fn]
//! pub fn high_level() -> Result<(), e![ErrC, *LowLevelError]> {
//!     // 1. Sees ErrC -> Adds variant
//!     // 2. Sees *LowLevelError -> Inspects LowLevelError, finds (ErrA, ErrB)
//!     // 3. Final HighLevelError contains variants: ErrA, ErrB, ErrC
//!
//!     low_level()?; // Bubbles up automatically
//!     Ok(())
//! }
//! ```
//!
//! The syntax below has the exact same effects, `*LowLevelError` is nothing more than syntatic sugar
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
//! # pub fn low_level() -> Result<(), e![ErrA, ErrB]> {
//! #     // Generates LowLevelError { ErrA(ErrA), ErrB(ErrB) }
//! #     Err(LowLevelError::ErrA(ErrA)) // You can also type Err(ErrA.into())
//! # }
//! #[skerry_fn]
//! pub fn high_level() -> Result<(), e![ErrA, ErrB, ErrC]> {
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
//! This attribute coordinates with `#[skerry_fn]` to split the generated code
//! so error enums are generated outside the `impl` block.
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
//! # pub fn remote_call() -> Result<(), e![ConnectionFailed]> {
//! #    Ok(())
//! # }
//! #
//! pub struct Database;
//!
//! #[skerry_impl(prefix(Database))] // Optional prefix for functions inside impl block
//! impl Database {
//!     #[skerry_fn]
//!     pub fn connect(&self) -> Result<(), e![*RemoteCallError]> {
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
//! ## Using Skerry inside Trait Blocks
//!
//! Skerry provides the `#[skerry_trait]` attribute to handle methods within `trait` blocks.
//! This attribute coordinates with `#[skerry_fn]` to split the generated code
//! so error enums are generated outside the `trait` block.
//!
//! ### Example
//!
//! ```rust
//! use skerry::*;
//!
//! # #[skerry_mod]
//! # mod errors {
//! #   pub struct ParseFailed;
//! # }
//! #
//!
//! #[skerry_impl(prefix(ToJson))] // Optional prefix for functions inside trait block
//! trait ToJson {
//!     #[skerry_fn]
//!     pub fn parse(&self) -> Result<(), e![*ParseFailed]>;
//! }
//! ```
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
pub use skerry_macros::{skerry_fn, skerry_impl, skerry_mod, skerry_trait};

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
    fn my_fn1() -> Result<(), e![ErrA, ErrB, ErrC]> {
        Err(MyFn1Error::ErrA(ErrA))
    }

    #[skerry_fn]
    fn my_fn2() -> Result<(), e![ErrE, ErrF, Outer]> {
        let r: Result<(), OuterErrorFromLib> = Err(OuterErrorFromLib);
        let _ = r?;

        Ok(())
    }

    #[skerry_fn]
    pub fn my_fn3() -> Result<(), e![ErrA, ErrB, ErrC, *MyFn2Error]> {
        my_fn2()?;
        my_fn1()?;
        Ok(())
    }

    pub struct MyStruct;

    #[skerry_impl(prefix(MyStruct))]
    impl MyStruct {
        #[skerry_fn]
        pub fn struct_fn() -> Result<(), e![*MyFn3Error]> {
            my_fn3()?;
            Ok(())
        }

        #[skerry_fn]
        pub fn struct_fn_2() -> Result<(), e![*MyStructStructFnError]> {
            Self::struct_fn()?;
            Ok(())
        }
    }

    #[skerry_trait(prefix(TestTrait))]
    trait TestTrait {
        #[skerry_fn]
        fn test() -> Result<(), e![ErrA, ErrB, *MyFn3Error]>;
    }

    #[skerry_impl]
    impl TestTrait for MyStruct {
        fn test() -> Result<(), TestTraitTestError> {
            Ok(())
        }
    }

    #[test]
    pub fn test() {
        let _ = my_fn3();
        let _ = MyStruct::test();
        let _ = MyStruct::struct_fn_2();
    }
}
