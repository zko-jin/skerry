pub trait SkerryError {}

pub trait MissingConvert<T> {}

pub trait Contains<T> {}
pub trait IsSubsetOf<T> {}

pub trait IsSupersetOf<T> {}

impl<T: IsSupersetOf<S>, S> IsSubsetOf<T> for S {}

// impl<T: IsSubsetOf<CompositeA> + NotCompositeA> From<T> for CompositeA {
//     fn from(value: T) -> Self {
//         todo!()
//     }
// }

// pub struct CompositeB {}

// impl Contains<A> for CompositeB {}
// impl Contains<B> for CompositeB {}
// impl Contains<C> for CompositeB {}
// impl<T: Contains<A> + Contains<B> + Contains<C>> IsSubsetOf<T> for CompositeB {}
