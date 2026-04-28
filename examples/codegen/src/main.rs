#![feature(negative_impls)]
#![feature(auto_traits)]

use skerry::{
    e,
    skerry_error,
};
mod errors;

fn main() {
    let _ = my_fn_1();
}

#[skerry_error]
pub struct ErrA;

#[skerry_error]
pub struct ErrB;

#[skerry_error]
pub struct ErrC {
    inner: u32,
}

pub struct OuterError;

#[skerry_error]
pub struct Outer(crate::OuterError);

fn my_fn_1() -> Result<(), e![ErrA, ErrB, Outer]> {
    let r: Result<(), OuterError> = Err(OuterError);
    // r?;
    Ok(())
}

pub fn my_fn_2() -> Result<(), e![ErrA, *MyFn1Error]> {
    my_fn_1()?;
    Ok(())
}

trait TestTrait {
    fn my_fn_1() -> Result<(), e![ErrA, ErrB, *MyFn2Error]> {
        my_fn_2()?;
        Ok(())
    }
}
