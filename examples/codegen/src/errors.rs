struct OuterError;

#[skerry_global]
pub enum GlobalErrors {
    ErrA,
    ErrB,
    ErrC,
    Outer(OuterError),
}
