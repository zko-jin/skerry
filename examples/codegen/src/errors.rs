use std::net::TcpStream;

use skerry::{
    skerry_global,
    skerry_internals::Contains,
};

use crate::errors::__skerry_private::IoMarker;

#[skerry_global]
pub enum Global {
    ErrA,
    ErrB,
    ErrD,
    ErrC {
        inner: u32,
    },
    #[from]
    Io(TcpStream),
}
