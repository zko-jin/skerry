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

#[e(ErrA, Outer)]
fn my_fn_1() -> Result<()> {
    let _r: Result<(), OuterError> = Err(OuterError);
    // r?;
    Ok(())
}

#[e(ErrB, *MyFn1Error)]
pub fn my_fn_2() -> Result<()> {
    // my_fn_1()?;
    Ok(())
}

#[allow(unused)]
trait TestTrait {
    // #[e(*MyFn2Error)]
    // fn my_fn_1() -> Result<()> {
    //     my_fn_2()?;
    //     Ok(())
    // }
}
