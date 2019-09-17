mod index;
mod tags;

pub use index::*;
pub use tags::*;

#[derive(Debug)]
pub struct RTag {
    pub name: Tag,
    pub value: RType,
}
