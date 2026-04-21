//! # Skerry: Super Kool ERRors Yoh
//!
//! Example:
//! ```rust
//! use skerry::*;
//!
//! // There can be only one #[sherry_mod] in your project
//! # struct ErrorFromLib;
//! #[skerry_mod]
//! pub mod errors {
//!     pub struct DatabaseError;
//!     pub struct AuthError;
//!     pub struct ValidationError;
//!
//!     pub struct InvalidParse;
//!
//!     #[from]
//!     pub struct LibError(ErrorFromLib);
//! }
//!
//! // Generates a CheckAuthError enum automatically
//! #[skerry_fn]
//! fn check_auth() -> Result<(), e![AuthError]> {
//!     Err(CheckAuthError::AuthError(AuthError))
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
//!     pub fn run() -> Result<(), e![LibError, *CheckAuthError]> {
//!         // AuthError is pulled in from check_auth via '*CheckAuthError'
//!         check_auth()?;
//!
//!         // Automatically bubble up library errors as long as an
//!         // error from `#[skerry_mod]` implements `From` for it.
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
//!     // Whenever you do not want to generate a new error just don't
//!     // use e![], this will instead reuse an existing error
//!     #[skerry_fn]
//!     fn to_json(&self) -> Result<(), ToJsonError> {
//!         Ok(())
//!     }
//! }
//!
//! // Manually define composite errors like this
//! define_error!(ManualDefine, [ValidationError, *ToJsonError]);
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
//! // Recommended to be pub for easier macro expansions
//! pub use skerry::*;
//! # struct LibError;
//!
//! #[skerry_mod]
//! mod errors {
//!     pub struct ErrA;
//!     pub struct ErrB;
//!     pub struct ErrC;
//!     pub struct DatabaseErr;
//!     #[from]
//!     pub struct OuterLibError(LibError);
//! }
//! ```
//!
//! You can also anotate with `#[from]` to automatically add conversions from the inner type.
//! This is only valid for tuple structs with a single element.
//!
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
//!     Err(LowLevelError::ErrA(ErrA))
//!     // You can also type Err(ErrA.into())
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
//!     // Sees ErrC -> Adds variant
//!     // Sees *LowLevelError -> Inspects LowLevelError, finds (ErrA, ErrB)
//!     // Final HighLevelError contains variants: ErrA, ErrB, ErrC
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
//! # Ok(())
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
//! # use skerry::*;
//! # #[skerry_mod]
//! # mod errors {
//! #   pub struct ConnectionFailed;
//! # }
//! # #[skerry_fn]
//! # pub fn remote_call() -> Result<(), e![ConnectionFailed]> {
//! #    Ok(())
//! # }
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
//! # use skerry::*;
//! # #[skerry_mod]
//! # mod errors {
//! #   pub struct ParseFailed;
//! # }
//! #[skerry_trait(prefix(ToJson))] // Optional prefix for functions inside trait block
//! trait ToJson {
//!     #[skerry_fn]
//!     fn parse(&self) -> Result<(), e![ParseFailed]>;
//! }
//! ```
//! # Manual Error Definitions
//!
//! Manually define composite errors using the `define_error!` macro.
//! This allows you to skip needing a function to define errors.
//!
//! ### Example
//! ```rust
//! # use skerry::*;
//! # #[skerry_mod]
//! # mod errors {
//! #   pub struct ErrorA;
//! #   pub struct ErrorB;
//! # }
//! define_error!(ManualDefine, [ErrorA, ErrorB]);
//!
//! #[skerry_fn]
//! fn my_func_with_custom_error() -> Result<(), ManualDefine> {
//!     Ok(())
//! }
//! ```
//! ---
//!
//! ## Compile-Time Safety
//!
//! Skerry uses a custom trait system (`MissingConvert`) to verify error bounds at
//! compile-time. If you try to use `?` on a function whose errors are not represented
//! in your current return tuple, the compiler will refuse to build.
#![cfg_attr(
    feature = "custom_result",
    feature(try_trait_v2),
    feature(const_convert),
    feature(const_trait_impl)
)]

mod helpers;
mod macros;
mod traits;
pub use skerry_macros::{define_error, skerry_fn, skerry_impl, skerry_mod, skerry_trait};

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

        #[from]
        pub struct Outer(OuterErrorFromLib);
    }

    #[skerry_fn]
    fn my_fn1() -> Result<(), e![ErrA, ErrB, ErrC]> {
        Err(MyFn1Error::ErrA(ErrA))
    }

    #[skerry_fn]
    fn my_fn2() -> Result<(), e![ErrE, ErrF, Outer]> {
        let r: std::result::Result<(), OuterErrorFromLib> =
            std::result::Result::Err(OuterErrorFromLib);
        let _ = r?;

        Ok(())
    }

    #[skerry_fn]
    pub fn my_fn3() -> Result<(), e![ErrA, ErrB, ErrC, *MyFn2Error]> {
        my_fn2()?;
        my_fn1()?;
        Ok(())
    }

    define_error!(DefineTest, [ErrA]);

    pub struct MyStruct;

    #[skerry_impl(prefix(MyStruct))]
    impl MyStruct {
        #[skerry_fn]
        pub fn struct_fn() -> Result<(), e![*DefineTest, *MyFn3Error]> {
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

        #[skerry_fn]
        fn test_2() -> Result<(), e![ErrA, ErrB, *MyFn3Error]> {
            Ok(())
        }
    }

    impl TestTrait for MyStruct {
        fn test() -> Result<(), TestTraitTestError> {
            Ok(())
        }
    }

    fn no_expand_func() -> Result<(), MyFn3Error> {
        let r: Result<(), MyFn2Error> = Err(MyFn2Error::ErrE(ErrE));
        Ok(r?)
    }

    #[test]
    pub fn test() {
        let _ = my_fn3();
        let _ = MyStruct::test();
        let _ = MyStruct::test_2();
        let _ = MyStruct::struct_fn_2();
        let _ = no_expand_func();
    }
}
