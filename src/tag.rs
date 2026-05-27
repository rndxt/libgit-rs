use std::io::{self, Write};

use crate::binary::BinaryReader;
use crate::object_db;
use crate::object_id::ObjectId;
use crate::object_type::ObjectType;
use crate::refs;
use crate::repo::Repository;
use crate::signature::Signature;

#[derive(Debug, Clone)]
pub struct Tag {
    pub object_id: ObjectId,
    pub object_type: ObjectType,
    pub tagger: Signature,
    pub name: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot create tag ref: {0}")]
    CannotCreateTag(#[from] refs::Error),

    #[error("lookup tag ref failed: {0}")]
    LookupTagRefFailed(refs::Error),

    #[error("cannot store tag object: {0}")]
    OdbWriteFailed(#[from] object_db::Error),

    #[error("cannot read tag object: {0}")]
    OdbReadFailed(object_db::Error),
}

pub fn create_annotated_tag(repo: &Repository, tag: &Tag) -> Result<ObjectId, Error> {
    let mut buffer = Vec::new();
    let mut writer = TagWriter::new(&mut buffer);
    writer.write_tag(tag).unwrap();

    let odb = repo.object_db();
    let tag_id = odb.write_raw(&buffer, ObjectType::Tag)?;

    let refs = repo.refs();
    let _ = refs.create_tag_ref(&tag.name, tag_id)?;
    Ok(tag_id)
}

pub fn create_light_tag(
    repo: &Repository,
    tag_name: &str,
    target_id: ObjectId,
) -> Result<(), Error> {
    let refs = repo.refs();
    let _ = refs.create_tag_ref(tag_name, target_id)?;
    Ok(())
}

pub fn lookup_tag_by_id(repo: &Repository, id: ObjectId) -> Result<Tag, Error> {
    let odb = repo.object_db();
    odb.read_tag(id).map_err(Error::OdbReadFailed)
}

pub fn lookup_tag_by_name(repo: &Repository, name: &str) -> Result<Tag, Error> {
    let tag_id = repo
        .refs()
        .lookup_tag_ref(name)
        .map_err(Error::LookupTagRefFailed)?
        .target;

    lookup_tag_by_id(repo, tag_id)
}

struct TagWriter<W: Write> {
    dest: W,
}

impl<W: Write> TagWriter<W> {
    fn new(dest: W) -> Self {
        Self { dest }
    }

    fn write_tag(&mut self, tag: &Tag) -> io::Result<()> {
        self.write_tag_ext(
            &tag.object_id,
            tag.object_type,
            &tag.name,
            &tag.tagger,
            &tag.message,
        )
    }

    fn write_tag_ext(
        &mut self,
        object_id: &ObjectId,
        object_type: ObjectType,
        tag_name: &str,
        tagger: &Signature,
        message: &str,
    ) -> io::Result<()> {
        writeln!(self.dest, "object {}", object_id)?;
        writeln!(self.dest, "type {}", object_type)?;
        writeln!(self.dest, "tag {}", tag_name)?;
        writeln!(self.dest, "tagger {}", tagger)?;
        writeln!(self.dest)?;
        writeln!(self.dest, "{}", message)?;
        Ok(())
    }
}

pub fn write_tag_to_buffer(tag: &Tag) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut writer = TagWriter::new(&mut buffer);
    writer.write_tag(tag).unwrap();
    buffer
}

pub fn read_tag_from_buffer(buffer: &[u8]) -> Option<Tag> {
    let mut reader = BinaryReader::new(buffer);
    reader.skip_prefix(b"object ")?;
    let object_id = reader
        .split_until_inclusive(b'\n')
        .and_then(|bytes| str::from_utf8(bytes).ok())
        .and_then(|str| ObjectId::from_str(str).ok())?;

    reader.skip_prefix(b"type ")?;
    let object_type = reader
        .split_until_inclusive(b'\n')
        .and_then(ObjectType::from)?;

    reader.skip_prefix(b"tag ")?;
    let name = reader
        .split_until_inclusive(b'\n')
        .and_then(|bytes| str::from_utf8(bytes).ok())
        .map(str::to_string)?;

    reader.skip_prefix(b"tagger ")?;
    let tagger = reader
        .split_until_inclusive(b'\n')
        .and_then(Signature::try_from_bytes)?;

    reader.skip_byte(b'\n')?;
    let message = reader
        .split_until_inclusive(b'\n')
        .and_then(|bytes| str::from_utf8(bytes).ok())
        .map(str::to_string)?;

    let tag = Tag {
        object_id,
        object_type,
        tagger,
        name,
        message,
    };
    Some(tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    use crate::time::Time;

    #[test]
    fn write_tag() -> testing::Result<()> {
        let object_id = ObjectId::from_str("85df50785d62d3b05ab03d9cbf7e4a0b49449730").unwrap();
        let object_type = ObjectType::Commit;
        let tag_name = "tag_name";
        let tagger =
            Signature::build("author", "author@email", Time::new(1771253662, 10800)).unwrap();
        let message = "message";

        let mut buffer = Vec::new();
        let mut writer = TagWriter::new(&mut buffer);
        writer.write_tag_ext(&object_id, object_type, tag_name, &tagger, message)?;

        let expected_data = b"object 85df50785d62d3b05ab03d9cbf7e4a0b49449730\ntype commit\ntag tag_name\ntagger author <author@email> 1771253662 +0300\n\nmessage\n";
        assert_eq!(expected_data, &buffer[..]);
        Ok(())
    }

    #[test]
    fn read_tag() -> testing::Result<()> {
        let data = b"object 85df50785d62d3b05ab03d9cbf7e4a0b49449730\ntype commit\ntag tag_name\ntagger author <author@email> 1771253662 +0300\n\nmessage\n";
        let tag = read_tag_from_buffer(data).unwrap();

        assert_eq!(
            tag.object_id,
            ObjectId::from_str("85df50785d62d3b05ab03d9cbf7e4a0b49449730").unwrap()
        );
        assert_eq!(tag.object_type, ObjectType::Commit);
        assert_eq!(tag.name, "tag_name");
        assert_eq!(
            tag.tagger,
            Signature {
                name: String::from("author"),
                email: String::from("author@email"),
                time: Time {
                    unix_time: 1771253662,
                    offset: 10800
                },
            }
        );
        assert_eq!(tag.message, "message");
        Ok(())
    }
}
