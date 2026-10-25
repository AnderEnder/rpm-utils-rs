use rpm_utils::RPMFile;
use rpm_utils::header::{RType, Tag, Tags, TagsWrite};
use rpm_utils::lead::Lead;
use std::io::{self, Cursor};

fn package(format: u32) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    Lead {
        major: 4,
        minor: 0,
        ..Lead::default()
    }
    .write(&mut bytes)?;
    bytes.write_header(&Tags::<rpm_utils::header::SignatureTag>::new())?;
    let mut tags = Tags::new();
    tags.insert(Tag::RpmFormat, RType::Int32(format));
    bytes.write_header(&tags)?;
    Ok(bytes)
}

#[test]
fn rejects_v6_even_when_it_uses_a_v4_lead() -> io::Result<()> {
    let error = RPMFile::read(Cursor::new(package(6)?)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    assert!(error.to_string().contains("v6"));
    let mut rpm = RPMFile::read(Cursor::new(package(4)?))?;
    rpm.header_tags.insert(Tag::RpmFormat, RType::Int32(6));
    rpm.file.set_position(0);
    let original = rpm.file.get_ref().clone();
    assert_eq!(
        rpm.write_head().unwrap_err().kind(),
        io::ErrorKind::Unsupported
    );
    assert_eq!(*rpm.file.get_ref(), original);
    Ok(())
}

#[test]
fn accepts_explicit_v3_and_v4_formats() -> io::Result<()> {
    for format in [3, 4] {
        let rpm = RPMFile::read(Cursor::new(package(format)?))?;
        assert_eq!(rpm.header_tags.get_as_u32(Tag::RpmFormat), format);
    }
    Ok(())
}
