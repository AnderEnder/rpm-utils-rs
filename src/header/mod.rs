mod index;
mod sigtags;
mod tags;

pub use index::*;
pub use sigtags::*;
pub use tags::*;

use num_traits::{FromPrimitive, ToPrimitive};

#[derive(Debug)]
pub struct RTag<T>
where
    T: FromPrimitive + Default,
{
    pub name: T,
    pub value: RType,
}
