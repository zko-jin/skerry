#![skerry]

extern crate self as skerry;
pub use skerry::*;

pub struct OuterErrorFromLib;

#[skerry_mod]
pub mod errors {
    pub struct ErrA;
    pub struct ErrB;
    pub struct ErrC;
    pub struct ErrD;
    pub struct ErrE;
    pub struct ErrF;

    #[from]
    pub struct Outer(OuterErrorFromLib);
}

fn my_fn1() -> Result<(), e![ErrA, ErrB, ErrC]> {
    Err(MyFn1Error::ErrA(ErrA))
}

fn my_fn2() -> Result<(), e![ErrE, ErrF, Outer]> {
    let r: std::result::Result<(), OuterErrorFromLib> = std::result::Result::Err(OuterErrorFromLib);
    let _ = r?;

    Ok(())
}

pub fn my_fn3() -> Result<(), e![ErrA, ErrB, ErrC, *MyFn2Error]> {
    my_fn2()?;
    my_fn1()?;
    Ok(())
}

define_error!(DefineTest, [ErrA]);

pub struct MyStruct;

#[skerry_impl(prefix(MyStruct))]
impl MyStruct {
    pub fn struct_fn() -> Result<(), e![*DefineTest, *MyFn3Error]> {
        my_fn3()?;
        Ok(())
    }

    pub fn struct_fn_2() -> Result<(), e![*MyStructStructFnError]> {
        Self::struct_fn()?;
        Ok(())
    }
}

#[skerry_trait(prefix(TestTrait))]
trait TestTrait {
    fn test() -> Result<(), e![ErrA, ErrB, *MyFn3Error]>;

    fn test_2() -> Result<(), e![ErrA, ErrB, *MyFn3Error]> {
        Ok(())
    }
}

impl TestTrait for MyStruct {
    fn test() -> Result<(), TestTraitTestError> {
        my_fn3()?;
        Ok(())
    }
}

fn no_expand_func() -> Result<(), MyFn3Error> {
    let r: Result<(), MyFn2Error> = Err(MyFn2Error::ErrE(ErrE));
    Ok(r?)
}

#[test]
pub fn test() {
    let _ = my_fn3();
    let _ = MyStruct::test();
    let _ = MyStruct::test_2();
    let _ = MyStruct::struct_fn_2();
    let _ = no_expand_func();
}
