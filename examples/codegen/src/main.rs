#![feature(negative_impls)]
#![feature(auto_traits)]

use skerry::{
    e,
    skerry_error,
    skerry_internals::{
        Contains,
        IsSubsetOf,
    },
};

use crate::errors::GlobalErrors;
mod errors;

fn main() {
    let _ = my_fn_1();
}

#[skerry_error]
pub struct ErrA;

#[skerry_error]
pub struct ErrB;

#[allow(unused)]
#[skerry_error]
pub struct ErrC {
    inner: u32,
}

pub struct OuterError;

#[skerry_error]
pub struct Outer(OuterError);

impl From<OuterError> for GlobalErrors {
    fn from(value: OuterError) -> Self {
        GlobalErrors::Outer(Outer(value))
    }
}
impl<T: Contains<Outer>> IsSubsetOf<T> for OuterError {}

fn my_fn_1() -> Result<(), e![ErrA, Outer]> {
    let r: Result<(), OuterError> = Err(OuterError);
    r?;
    Ok(())
}

fn my_fn_2_1() -> Result<(), crate::errors::MyFn2Error> {
    // let r: Result<(), Outer> = Err(Outer(OuterError));
    // r?;
    my_fn_1()?;
    Ok(())
}

pub fn my_fn_2() -> Result<(), e![ErrB, *MyFn1Error]> {
    // my_fn_1()?;
    Ok(())
}

#[allow(unused)]
trait TestTrait {
    fn my_fn_1() -> Result<(), e![ErrA, ErrB, *MyFn2Error]> {
        my_fn_2()?;
        Ok(())
    }
}
