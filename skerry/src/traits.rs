/// Do NOT implement this type manually.
pub trait Contains<T> {}
/// Do NOT implement this type manually.
pub trait IsSubsetOf<T> {}
/// Do NOT implement this type manually.
pub trait IsSupersetOf<T> {}

impl<T: IsSubsetOf<S>, S> IsSupersetOf<T> for S {}
