use rpm_utils::header::{HeaderLead, Index, IndexArray, RType, Tag, Tags, TagsWrite, Type};
use std::io::{self, Cursor};

#[test]
fn reads_final_strings_without_relying_on_index_order() -> io::Result<()> {
    let indexes = [
        Index {
            tag: Tag::Version,
            itype: Type::String,
            offset: 5,
            count: 1,
        },
        Index {
            tag: Tag::Name,
            itype: Type::String,
            offset: 0,
            count: 1,
        },
        Index {
            tag: Tag::DirNames,
            itype: Type::StringArray,
            offset: 9,
            count: 2,
        },
    ];
    let data = b"name\x001.0\x00/a/\x00/b/\x00";
    let tags = Tags::read(&mut Cursor::new(data), &indexes, data.len())?;
    assert_eq!(tags.get_as_string(Tag::Name), "name");
    assert_eq!(tags.get_as_string(Tag::Version), "1.0");
    assert_eq!(tags.get_as_string_array_or(Tag::DirNames), ["/a/", "/b/"]);
    Ok(())
}

#[test]
fn rejects_invalid_offsets_counts_and_unterminated_strings() {
    for (itype, offset, count, data) in [
        (Type::Int8, usize::MAX, 1, vec![0]),
        (Type::Int64, 0, usize::MAX, vec![0]),
        (Type::Int32, 0, 2, vec![0; 4]),
        (Type::Bin, 1, usize::MAX, vec![0]),
        (Type::String, 0, 1, b"unterminated".to_vec()),
        (Type::StringArray, 0, 2, b"one\x00".to_vec()),
    ] {
        let index = Index {
            tag: Tag::Name,
            itype,
            offset,
            count,
        };
        let error = Tags::read(&mut Cursor::new(&data), &[index], data.len()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
    let result = IndexArray::read::<_, Tag>(&mut Cursor::new([]), usize::MAX);
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
}

#[test]
fn header_writer_uses_correct_types_alignment_and_no_trailing_padding() -> io::Result<()> {
    let mut tags = Tags::new();
    tags.insert(Tag::Name, RType::String("ab".into()))
        .insert(Tag::Epoch, RType::Int32(7))
        .insert(Tag::Size, RType::Int64(42))
        .insert(Tag::FileMD5s, RType::Bin(vec![0, 1, 255]))
        .insert(Tag::Version, RType::Char('x'))
        .insert(Tag::FileSizes, RType::Int32Array(vec![]));
    let mut bytes = Vec::new();
    bytes.write_header(&tags)?;
    let mut reader = Cursor::new(bytes);
    let lead = HeaderLead::read(&mut reader)?;
    assert_eq!(
        reader.get_ref().len(),
        16 + lead.nindex * 16 + lead.hsize as usize
    );
    // Raw index order is by tag, independent of data alignment or HashMap iteration order.
    let raw: Vec<_> = (0..lead.nindex)
        .map(|_| Index::<Tag>::read(&mut reader))
        .collect::<io::Result<_>>()?;
    assert!(
        raw.windows(2)
            .all(|pair| pair[0].tag as u32 <= pair[1].tag as u32)
    );
    for index in &raw {
        let alignment = match index.itype {
            Type::Int16 => 2,
            Type::Int32 => 4,
            Type::Int64 => 8,
            _ => 1,
        };
        assert_eq!(index.offset % alignment, 0);
    }
    let parsed = Tags::read(&mut reader, &raw, lead.hsize as usize)?;
    assert_eq!(parsed.0, tags.0);
    Ok(())
}

#[test]
fn size_and_timestamp_helpers_use_the_rpm_numeric_types() {
    let mut tags = Tags::new();
    tags.insert_size(42).insert_build_time(123456);
    assert_eq!(tags.get_value(Tag::Size), Some(RType::Int32(42)));
    assert_eq!(tags.get_value(Tag::BuildTime), Some(RType::Int32(123456)));
    tags.insert_size(u64::from(u32::MAX) + 1);
    assert!(tags.get_value(Tag::Size).is_none());
    assert_eq!(
        tags.get_value(Tag::LongSize),
        Some(RType::Int64(u64::from(u32::MAX) + 1))
    );
    tags.insert_size(1);
    assert!(tags.get_value(Tag::LongSize).is_none());
    let mut signatures = Tags::new();
    signatures.insert_payload_size(42);
    assert_eq!(
        signatures.get_value(rpm_utils::header::SignatureTag::PayloadSize),
        Some(RType::Int32(42))
    );
}
