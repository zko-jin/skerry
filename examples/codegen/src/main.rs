#![feature(negative_impls)]
#![feature(auto_traits)]

use std::net::TcpStream;

use skerry::skerry;

mod errors;

fn main() {
    let _ = my_fn_1();
}

#[skerry]
fn my_fn_1() -> Result<(), e![ErrA, Io]> {
    let r: Result<(), TcpStream> = Err(TcpStream::connect("addr").unwrap());
    r?;
    Ok(())
}

#[skerry]
pub fn my_fn_3() -> Result<(), e![ErrB, *MyFn1Error]> {
    my_fn_1()?;
    Ok(())
}

#[skerry]
#[allow(unused)]
trait TestTrait {
    fn my_fn_5() -> Result<(), e![*MyFn3Error]> {
        my_fn_3()?;
        Ok(())
    }
}
