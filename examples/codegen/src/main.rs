#![feature(negative_impls)]
#![feature(auto_traits)]

use skerry::skerry;

use crate::errors::OuterError;

// use crate::errors::GlobalErrors;

mod errors;

fn main() {
    // let _ = my_fn_1();
}

// impl From<OuterError> for GlobalErrors {
//     fn from(value: OuterError) -> Self {
//         GlobalErrors::Outer(value)
//     }
// }
// impl<T: Contains<Outer>> IsSubsetOf<T> for OuterError {}

#[skerry]
fn my_fn_1() -> Result<(), e![ErrA, Outer]> {
    let _r: Result<(), OuterError> = Err(OuterError);
    // r?;
    Ok(())
}

// #[skerry]
// pub fn my_fn_3() -> Result<(), e![ErrB, *MyFn1Error]> {
//     my_fn_1()?;
//     Ok(())
// }

// #[skerry]
// trait TestTrait {
//     // #[e(*MyFn3Error)]
//     // fn my_fn_5() -> Result<()> {
//     //     my_fn_3()?;
//     //     Ok(())
//     // }
// }
