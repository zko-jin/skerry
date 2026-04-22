use skerry::{
    e,
    skerry_error,
};

skerry::include!();

fn main() {
    let _ = test();
}

#[skerry_error]
struct ErrA;

#[skerry_error]
struct ErrB;

fn test() -> Result<(), e![ErrA, ErrB]> {
    Ok(())
}
