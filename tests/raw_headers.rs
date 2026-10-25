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

#[test]
fn overlapping_values_cannot_expand_parser_memory_without_bound() -> io::Result<()> {
    let length = 17 * 1024 * 1024_u32;
    let mut bytes = Vec::new();
    rpm_utils::header::HeaderLead::from(2, length).write(&mut bytes)?;
    for id in [65000_u32, 65001] {
        for value in [id, 7, 0, length] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
    }
    bytes.resize(bytes.len() + length as usize, 0);
    assert_eq!(
        RawHeader::read(&mut bytes.as_slice()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    Ok(())
}
