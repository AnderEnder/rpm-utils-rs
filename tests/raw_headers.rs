use rpm_utils::header::{RawHeader, RawTag, Type};
use std::io;

#[test]
fn preserves_unknown_tags_locales_and_char_arrays() -> io::Result<()> {
    let mut header = RawHeader::default();
    header.set(RawTag::new(65000, Type::Bin, 4, vec![0, 255, 1, 0])?)?;
    header.set(RawTag::new(65001, Type::Char, 3, vec![1, 2, 3])?)?;
    header.set(RawTag::strings_value(
        1004,
        Type::I18nstring,
        &["English".into(), "Українська".into()],
    )?)?;
    let bytes = header.encode_region(63)?;
    let read = RawHeader::read(&mut bytes.as_slice())?;
    assert_eq!(read.get(65000), header.get(65000));
    assert_eq!(read.get(65001), header.get(65001));
    assert_eq!(read.strings(1004)?, ["English", "Українська"]);
    assert_eq!(read.encode_region(63)?, bytes);
    Ok(())
}

#[test]
fn refuses_ambiguous_duplicates_and_invalid_lengths() -> io::Result<()> {
    assert!(RawTag::new(1, Type::Int64, u32::MAX, vec![]).is_err());
    assert!(RawTag::new(1, Type::StringArray, 2, b"one\0".to_vec()).is_err());
    let tag = RawTag::new(65000, Type::Bin, 1, vec![0])?;
    let header = RawHeader {
        entries: vec![tag.clone(), tag],
        ..Default::default()
    };
    assert!(header.encode_region(63).is_err());
    Ok(())
}
