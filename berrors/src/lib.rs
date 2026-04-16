#![feature(const_trait_impl)]
#![allow(unused_features)]
#![feature(try_trait_v2)]

mod helpers;
mod macros;
mod result;
mod traits;

pub use proc_berrors::{error_module, fn_error};

pub mod berrors_internals {
    pub use crate::{helpers::*, macros::*, traits::*};
    pub use proc_berrors::*;
}

// #[cfg(test)]
mod test {
    extern crate self as berrors;
    pub use berrors::*;

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
        my_fn1();
        Ok(())
    }

    #[test]
    pub fn test() {
        my_fn3();
    }

    // struct TestStruct;

    // impl TestStruct {
    //     #[fn_error]
    //     fn new_and_stuff() -> Result<(), (ErrA, ErrB, ErrC)> {
    //         todo!()
    //     }
    // }
}
