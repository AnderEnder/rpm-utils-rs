mod index;
mod sigtags;
mod tags;

pub use index::*;
pub use sigtags::*;
pub use tags::*;

use num_traits::{FromPrimitive, ToPrimitive};
use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug, Clone)]
pub struct RTag<T>
where
    T: FromPrimitive + Default,
{
    pub name: T,
    pub value: RType,
}

type Tags<T> = HashMap<T, RType>;

pub fn get_tag<T>(tags: &Tags<T>, name: &T) -> String
where
    T: FromPrimitive + Default + Eq + Hash
{
    match tags.get(name) {
        Some(value) => value.into(),
        _ => "".to_string(),
    }
}

pub fn get_tag_i32<T>(tags: &Tags<T>, name: &T) -> i32
where
    T: FromPrimitive + Default + Eq + Hash
{
    match tags.get(name) {
        Some(value) => value.into(),
        _ => i32::default(),
    }
}
