mod index;
mod sigtags;
mod tags;

pub use index::*;
pub use sigtags::*;
pub use tags::*;

use num_traits::{FromPrimitive, ToPrimitive};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RTag<T>
where
    T: FromPrimitive + Default,
{
    pub name: T,
    pub value: RType,
}

type Tags = HashMap<Tag, RType>;

pub fn get_tag(tags: &Tags, name: &Tag) -> String {
    match tags.get(name) {
        Some(value) => value.into(),
        _ => "".to_string(),
    }
}

pub fn get_tag_i32(tags: &Tags, name: &Tag) -> i32 {
    match tags.get(name) {
        Some(value) => value.into(),
        _ => i32::default(),
    }
}
