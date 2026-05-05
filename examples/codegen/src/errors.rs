use skerry::{
    skerry,
    skerry_global,
};

pub struct OuterError;

#[skerry_global]
pub enum GlobalErrors {
    ErrA,
    ErrB,
    ErrC,
    #[from]
    Outer(OuterError),
}

#[skerry]
#[allow(unused)]
pub fn my_fn_1() -> Result<(), e![ErrA, ErrB, ErrC, Outer]> {
    let x: Result<(), OuterError> = Err(OuterError);
    x?;
    Ok(())
}

#[skerry]
#[allow(unused)]
pub fn my_fn_2(err: GlobalErrors) -> Result<(), e![ErrA, *MyFn1Error]> {
    my_fn_1()?;
    Ok(())
}
