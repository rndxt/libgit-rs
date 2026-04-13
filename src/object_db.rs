use std::fs::{self, File, Metadata};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

use crate::object_id::ObjectId;
use crate::object_type::ObjectType;
use crate::sha1::Sha1Hasher;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot create tmp file: {0}")]
    CreateTmpFileFailed(io::Error),

    #[error("cannot remove tmp file: {0}")]
    RemoveTmpFile(io::Error),

    #[error("cannot rename tmp file to {0}: {1}")]
    RenameFile(PathBuf, io::Error),

    #[error("cannot create dir: {0}")]
    CreateDirFailed(io::Error),

    #[error("cannot write to file: {0}")]
    WriteToFileFailed(io::Error),

    #[error("cannot load object: {0}")]
    CannotLoadObject(io::Error),

    #[error("hash mismatch")]
    HashMismatch,

    #[error("object is corrupted")]
    CorruptedObject,
}

pub fn hash_buffer(buffer: &[u8], object_type: ObjectType) -> ObjectId {
    let mut hasher = Sha1Hasher::new();
    let header = build_object_header(object_type, buffer.len() as u64);
    hasher.update(&header);
    hasher.update(buffer);
    ObjectId::from_sha1(hasher.finalize())
}

pub fn hash_file<P: AsRef<Path>>(path: P, object_type: ObjectType) -> io::Result<ObjectId> {
    let mut file = File::open(path.as_ref())?;
    let mut size = file.metadata()?.len();

    let mut hasher = Sha1Hasher::new();
    hasher.update(build_object_header(object_type, size));

    let mut buffer = [0u8; 0x10000];
    while size > 0 {
        let n = file.read(&mut buffer[..])?;

        if n == 0 {
            return Err(ErrorKind::UnexpectedEof.into());
        }

        let n64 = n as u64;
        if n64 > size {
            return Err(ErrorKind::FileTooLarge.into());
        }

        hasher.update(&buffer[..n]);
        size -= n64;
    }

    let id = ObjectId::from_sha1(hasher.finalize());
    Ok(id)
}

pub struct ObjectDB {
    objects_dir: PathBuf,
}

impl ObjectDB {
    pub fn new(objects_dir: &Path) -> ObjectDB {
        Self {
            objects_dir: objects_dir.to_path_buf(),
        }
    }

    pub fn objects_dir(&self) -> &Path {
        &self.objects_dir
    }

    pub fn write_raw(&self, buffer: &[u8], object_type: ObjectType) -> Result<ObjectId, Error> {
        self.write_loose_object(buffer, buffer.len() as u64, object_type)
    }

    pub fn write_file(
        &self,
        file: &File,
        stat: &Metadata,
        object_type: ObjectType,
    ) -> Result<ObjectId, Error> {
        self.write_loose_object(file, stat.len(), object_type)
    }

    fn write_loose_object<R: Read>(
        &self,
        data: R,
        size: u64,
        object_type: ObjectType,
    ) -> Result<ObjectId, Error> {
        let path_tmp = self.objects_dir().join("tmp");
        let tmp_file = File::create(&path_tmp).map_err(Error::CreateTmpFileFailed)?;

        let id = ObjectWriter::new(tmp_file)
            .write_object(data, size, object_type)
            .map_err(Error::WriteToFileFailed)?;

        let hex_id = id.to_string();
        let mut odb_path = self.objects_dir().join(&hex_id[..2]);
        fs::create_dir_all(&odb_path).map_err(Error::CreateDirFailed)?;

        odb_path.push(&hex_id[2..]);
        // Assume that if file already exists, then it is correct.
        if let Err(e) = fs::hard_link(&path_tmp, &odb_path)
            && e.kind() != ErrorKind::AlreadyExists
        {
            return Err(Error::RenameFile(odb_path, e));
        }

        fs::remove_file(path_tmp).map_err(Error::RemoveTmpFile)?;
        Ok(id)
    }

    pub fn load_object(&self, id: ObjectId) -> Result<(ObjectType, Vec<u8>), Error> {
        let hex = id.to_string();
        let mut path = PathBuf::from(self.objects_dir());
        path.push(&hex[..2]);
        path.push(&hex[2..]);

        let file = File::open(path).map_err(Error::CannotLoadObject)?;
        let mut decoder = ZlibDecoder::new(file);
        let mut data = Vec::new();
        decoder
            .read_to_end(&mut data)
            .map_err(Error::CannotLoadObject)?;

        let mut hasher = Sha1Hasher::new();
        hasher.update(&data);
        if ObjectId::from_sha1(hasher.finalize()) != id {
            return Err(Error::HashMismatch);
        }

        parse_object_type_and_content(&data)
    }
}

fn parse_object_type_and_content(data: &[u8]) -> Result<(ObjectType, Vec<u8>), Error> {
    let idx = data
        .iter()
        .position(|b| *b == b' ')
        .ok_or(Error::CorruptedObject)?;
    let (header, rest) = data.split_at(idx);
    let object_type = ObjectType::from(header).ok_or(Error::CorruptedObject)?;

    let rest = &rest[1..];
    let idx = rest
        .iter()
        .position(|b| *b == b'\0')
        .ok_or(Error::CorruptedObject)?;
    let (size, rest) = rest.split_at(idx);
    let size = str::from_utf8(size).map_err(|_| Error::CorruptedObject)?;
    let size = usize::from_str_radix(size, 10).map_err(|_| Error::CorruptedObject)?;
    if size != rest[1..].len() {
        return Err(Error::CorruptedObject);
    }

    Ok((object_type, rest[1..].to_vec()))
}

struct ObjectWriter<W: Write> {
    encoder: ZlibEncoder<W>,
    hasher: Sha1Hasher,
}

impl<W: Write> Write for ObjectWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.encoder.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.encoder.flush()
    }
}

impl<W: Write> ObjectWriter<W> {
    fn new(dst: W) -> Self {
        Self {
            encoder: ZlibEncoder::new(dst, Compression::fast()),
            hasher: Sha1Hasher::new(),
        }
    }

    fn write_object<R: Read>(
        mut self,
        mut content: R,
        size: u64,
        object_type: ObjectType,
    ) -> io::Result<ObjectId> {
        let header = build_object_header(object_type, size);
        io::copy(&mut &header[..], &mut self)?;
        io::copy(&mut content, &mut self)?;
        let id = ObjectId::from_sha1(self.hasher.finalize());
        Ok(id)
    }
}

fn build_object_header(object_type: ObjectType, size: u64) -> Vec<u8> {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(object_type.as_bytes());
    buffer.push(b' ');
    buffer.extend_from_slice(size.to_string().as_bytes());
    buffer.push(b'\0');
    buffer
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn hash_buffer_blob() -> testing::Result<()> {
        let data = b"abcd";
        let expected_oid = "85df50785d62d3b05ab03d9cbf7e4a0b49449730";
        let expected_oid = ObjectId::from_str(expected_oid).unwrap();

        let actual_oid = hash_buffer(data, ObjectType::Blob);
        assert_eq!(expected_oid, actual_oid);
        Ok(())
    }

    #[test]
    fn write_object() -> testing::Result<()> {
        let data = b"abcd";
        let expected_oid = "85df50785d62d3b05ab03d9cbf7e4a0b49449730";
        let expected_oid = ObjectId::from_str(expected_oid).unwrap();
        let expected_buffer =
            b"\x78\x01\x4b\xca\xc9\x4f\x52\x30\x61\x48\x4c\x4a\x4e\x01\x00\x15\x5c\x03\x7e";

        let mut buffer = Vec::new();
        let writer = ObjectWriter::new(&mut buffer);
        let actual_oid = writer.write_object(&data[..], data.len() as u64, ObjectType::Blob)?;

        assert_eq!(expected_oid, actual_oid);
        assert_eq!(expected_buffer, &buffer[..]);
        Ok(())
    }

    #[test]
    fn read_object() -> testing::Result<()> {
        let data = b"blob 4\0abcd";
        let (object_type, content) = parse_object_type_and_content(data)?;
        assert_eq!(object_type, ObjectType::Blob);
        assert_eq!(content, b"abcd");
        Ok(())
    }
}
