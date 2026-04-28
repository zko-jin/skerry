pub trait SkerryError {}

pub trait MissingConvert<T> {}

pub trait Contains<T> {}
pub trait IsSubsetOf<T> {}
pub trait IsSupersetOf<T> {}

impl<T: IsSupersetOf<S>, S> IsSubsetOf<T> for S {}
