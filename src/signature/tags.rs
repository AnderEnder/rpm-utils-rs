#[derive(Debug)]
pub enum Tag {
    HeaderSignatures,
    HeaderImmutable,
    Headeri18Ntable,
    Size,
    Other(i32),
}

impl From<i32> for Tag {
    fn from(tag: i32) -> Self {
        match tag {
            62 => Tag::HeaderSignatures,
            63 => Tag::HeaderImmutable,
            100 => Tag::Headeri18Ntable,
            1000 => Tag::Size,
            x => Tag::Other(x),
        }
    }
}
