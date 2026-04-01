use std::fs::{self, File, Metadata};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::ZlibEncoder;

use crate::object_id::ObjectId;
use crate::object_type::ObjectType;
use crate::sha1::Sha1Hasher;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("fail to create tmp file: {0}")]
    CreateTmpFileFailed(io::Error),

    #[error("fail to remove tmp file: {0}")]
    RemoveTmpFile(io::Error),

    #[error("fail to rename tmp file to {0}: {1}")]
    RenameFile(PathBuf, io::Error),

    #[error("fail to create dir {0}: {1}")]
    CreateDirFailed(PathBuf, io::Error),

    #[error("error writing to file: {0}")]
    WriteToFileFailed(io::Error),
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
        let (tmp_file, path_tmp) = self.create_tmpfile()?;

        let writer = ObjectWriter::new(tmp_file);
        let id = match writer.write_object(data, size, object_type) {
            Ok(id) => id,
            Err(e) => return Err(Error::WriteToFileFailed(e)),
        };

        let hex_id = id.to_string();
        let mut odb_path = self.objects_dir().to_path_buf();
        odb_path.push(&hex_id[..2]);
        if let Err(e) = fs::create_dir(&odb_path)
            && e.kind() != ErrorKind::AlreadyExists
        {
            return Err(Error::CreateDirFailed(odb_path, e));
        }

        odb_path.push(&hex_id[2..]);

        // TODO: Assume that if file already exists, then it is correct.
        // Git does the same. But now for testing delete before write.
        // if !Path::try_exists(&odb_path)? {
        let _ = fs::remove_file(&odb_path);
        if let Err(e) = fs::hard_link(&path_tmp, &odb_path) {
            return Err(Error::RenameFile(odb_path, e));
        }

        if let Err(e) = fs::remove_file(path_tmp) {
            return Err(Error::RemoveTmpFile(e));
        }
        // }

        Ok(id)
    }

    fn create_tmpfile(&self) -> Result<(File, PathBuf), Error> {
        let path = self.objects_dir().join("tmp");
        match File::create(path.as_path()) {
            Ok(file) => Ok((file, path)),
            Err(e) => Err(Error::CreateTmpFileFailed(e)),
        }
    }
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
}
