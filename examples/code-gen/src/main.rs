#![feature(try_trait_v2)]
use skerry::{
    e,
    skerry_error,
};

skerry::include!();

fn main() {
    let _ = test();
}

#[skerry_error]
pub struct ErrA;

#[skerry_error]
pub struct ErrB;

fn test() -> Result<(), e![ErrA, ErrB]> {
    Ok(())
}
fn test_2() -> Result<(), e![ErrA, *TestError]> {
    Ok(())
}
