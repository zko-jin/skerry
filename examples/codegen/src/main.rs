#![feature(try_trait_v2)]

use skerry::{
    e,
    skerry_error,
};

skerry::include!();

fn main() {
    let _ = my_fn_1();
}

#[skerry_error]
pub struct ErrA;

#[skerry_error]
pub struct ErrB;

fn my_fn_1() -> errors::Result<(), e![ErrA, ErrB]> {
    errors::Ok(())
}

fn my_fn_2() -> errors::Result<(), e![ErrA, *MyFn1Error]> {
    my_fn_1()?;
    errors::Ok(())
}

trait TestTrait {
    fn my_fn_1() -> errors::Result<(), e![ErrA, ErrB, *MyFn2Error]> {
        errors::Ok(())
    }
}
