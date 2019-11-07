mod index;
mod sigtags;
mod tags;

pub use index::*;
pub use sigtags::*;
pub use tags::*;

use num_traits::{FromPrimitive, ToPrimitive};
use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug, Default)]
pub struct Tags<T>(pub HashMap<T, RType>)
where
    T: Eq + Hash;

impl<T> Tags<T>
where
    T: FromPrimitive + Default + Eq + Hash,
{
    pub fn get<O>(&self, name: T) -> O
    where
        O: Default + From<RType>,
    {
        match self.0.get(&name) {
            Some(value) => value.clone().into(),
            _ => O::default(),
        }
    }
}
