use skerry::skerry_global;

pub struct OuterError;

#[skerry_global]
pub enum Global {
    ErrA,
    ErrB,
    ErrD,
    ErrC {
        inner: u32,
    },
    #[from]
    Outer(OuterError),
}
