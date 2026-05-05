#![feature(negative_impls)]
#![feature(auto_traits)]

mod errors;

fn main() {
    // let _ = my_fn_1();
}

// #[skerry]
// fn my_fn_1() -> Result<(), e![Outer, *MyFn3Error]> {
//     let _r: Result<(), OuterError> = Err(OuterError);
//     // r?;
//     Ok(())
// }

// #[skerry]
// pub fn my_fn_3() -> Result<(), e![ErrB, *MyFn1Error]> {
//     my_fn_1()?;
//     Ok(())
// }

// #[allow(unused)]
// #[skerry]
// trait TestTrait {
//     fn my_fn_5() -> Result<(), e![*MyFn3Error]> {
//         my_fn_3()?;
//         Ok(())
//     }
// }
