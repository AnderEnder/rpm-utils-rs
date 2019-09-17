mod index;
mod tags;
mod sigtags;

pub use index::*;
pub use tags::*;
pub use sigtags::*;

use num_traits::{FromPrimitive, ToPrimitive};

#[derive(Debug)]
pub struct RTag<T> where T: FromPrimitive + Default {
    pub name: T,
    pub value: RType,
}
