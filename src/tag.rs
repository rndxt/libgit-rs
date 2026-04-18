use std::io::{self, Write};

use crate::{object_db, refs};
use crate::object_id::ObjectId;
use crate::object_type::ObjectType;
use crate::repo::Repository;
use crate::signature::Signature;

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

    #[error("cannot store tag object: {0}")]
    OdbWriteFailed(#[from] object_db::Error),
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
        writeln!(self.dest, "object {}", object_id.to_string())?;
        writeln!(self.dest, "type {}", object_type)?;
        writeln!(self.dest, "tag {}", tag_name)?;
        writeln!(self.dest, "tagger {}", tagger)?;
        writeln!(self.dest)?;
        writeln!(self.dest, "{}", message)?;
        Ok(())
    }

    fn done(self) -> W {
        self.dest
    }
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
}
