mod index;
mod sigtags;
mod tags;

pub use index::*;
pub use sigtags::*;
pub use tags::*;

use num_traits::{FromPrimitive, ToPrimitive};
use std::collections::HashMap;
use std::hash::Hash;

pub type Tags<T> = HashMap<T, RType>;

pub fn get_tag<T, O>(tags: &Tags<T>, name: T) -> O
where
    T: FromPrimitive + Default + Eq + Hash,
    O: Default + From<RType>,
{
    match tags.get(&name) {
        Some(value) => value.clone().into(),
        _ => O::default(),
    }
}
