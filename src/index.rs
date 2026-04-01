use std::fs::{self, File, Metadata};
use std::io::{self, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use crate::ObjectId;
use crate::Repository;
use crate::object_db;
use crate::sha1::SHA1_SIZE_IN_BYTES;
use crate::sha1::Sha1;
use crate::sha1::Sha1Hasher;
use crate::{GIT_MODE_BLOB, GIT_MODE_BLOB_EXECUTABLE, GIT_MODE_LINK, ObjectType};

const INDEX_SIGNATURE: &[u8; 4] = b"DIRC";
const INDEX_HEADER_SIZE: usize = 12;
const EXTENSION_SIGNATURE_SIZE: usize = 4;

// const CACHE_TREE: &[u8; 4] = b"TREE";
// const RESOLVE_UNDO: &[u8; 4] = b"REUC";
// const RESOLVE_SPLIT_INDEX: &[u8; 4] = b"link";
// const UNTRACKED_CACHE: &[u8; 4] = b"UNTR";
// const FILE_SYSTEM_MONITOR_CACHE: &[u8; 4] = b"FSMN";
// const END_OF_INDEX_ENTRY: &[u8; 4] = b"EOIE";
// const INDEX_ENTRY_OFFSET_TABLE: &[u8; 4] = b"IEOT";
// const SPARSE_DIRECTORY_ENTRIES: &[u8; 4] = b"sdir";

#[derive(Debug)]
pub struct Index {
    version: u32,
    // TODO: Git use jagged array
    pub entries: Vec<IndexEntry>,
}

#[derive(Debug)]
pub struct IndexEntry {
    pub mtime: IndexTime,
    pub ctime: IndexTime,

    pub dev: u32,
    pub ino: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u32,

    pub id: ObjectId,
    pub flags: u16,
    pub path: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub struct IndexTime {
    pub seconds: u32,
    pub nanoseconds: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ParsingError {
    #[error("index file too short")]
    TooShort,

    #[error("invalid index signature")]
    InvalidSignature,

    #[error("unsupported index version: {0}")]
    UnsupportedVersion(u32),

    #[error("unsupported index extension: {0}")]
    UnsupportedExtension(String),

    #[error("entry {0} parse failed")]
    EntryParseFail(u32),

    #[error("invalid index checksum")]
    InvalidCheckSum,

    #[error("stage entries is unordered")]
    WrongEntriesOrder,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenIndexError {
    #[error("open index file failed")]
    FileNotFound(std::io::Error),

    #[error("invalid index: {0}")]
    ParseFailed(ParsingError),

    #[error("read index file failed: {0}")]
    ReadFileFailed(std::io::Error),
}

impl Index {
    pub fn new() -> Index {
        Self {
            version: 0,
            entries: vec![],
        }
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Index, OpenIndexError> {
        let mut index_file = match File::open(path) {
            Ok(file) => file,
            Err(e) => return Err(OpenIndexError::FileNotFound(e)),
        };

        // On files > 32 KB, Git uses mmap(2) call.
        // libgit2 always write all to memory.
        // For convenience, do same.
        let mut buffer = Vec::new();
        if let Err(e) = index_file.read_to_end(&mut buffer) {
            return Err(OpenIndexError::ReadFileFailed(e));
        };

        let mut parser = IndexParser::new(&buffer);
        let index = match parser.parse_index() {
            Ok(index) => index,
            Err(e) => return Err(OpenIndexError::ParseFailed(e)),
        };

        Ok(index)
    }

    pub fn find_by_path<P: AsRef<Path>>(&self, path: P) -> Result<usize, usize> {
        debug_assert!(self.is_entries_sorted());
        let path_raw = path.as_ref().as_os_str().as_encoded_bytes();
        self.find_by_raw_path(path_raw)
    }

    fn find_by_raw_path(&self, path_raw: &[u8]) -> Result<usize, usize> {
        debug_assert!(self.is_entries_sorted());
        // TODO: stage bit
        self.entries
            .binary_search_by(|entry| entry.path[..].cmp(path_raw))
    }

    pub fn add(&mut self, entry: IndexEntry) {
        debug_assert!(self.is_entries_sorted());
        match self.find_by_raw_path(&entry.path[..]) {
            Ok(i) => {
                self.entries[i] = entry;
            },
            Err(i) => {
                self.entries.insert(i, entry);
            },
        };
    }

    pub fn remove(&mut self, i: usize) {
        debug_assert!(i < self.count_entries());
        debug_assert!(self.is_entries_sorted());
        self.entries.remove(i);
    }

    pub fn get_unchecked(&self, i: usize) -> &IndexEntry {
        debug_assert!(i < self.count_entries());
        debug_assert!(self.is_entries_sorted());
        &self.entries[i]
    }

    pub fn get_mut_unchecked(&mut self, i: usize) -> &mut IndexEntry {
        debug_assert!(i < self.count_entries());
        debug_assert!(self.is_entries_sorted());
        &mut self.entries[i]
    }

    pub fn count_entries(&self) -> usize {
        debug_assert!(self.is_entries_sorted());
        self.entries.len()
    }

    fn is_entries_sorted(&self) -> bool {
        self.entries.is_sorted_by(|a, b| a.path < b.path)
    }
}

struct IndexParser<'a> {
    data: &'a [u8],
}

impl<'a> IndexParser<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    fn parse_index(&mut self) -> Result<Index, ParsingError> {
        if self.data.len() < INDEX_HEADER_SIZE + SHA1_SIZE_IN_BYTES {
            return Err(ParsingError::TooShort);
        }

        self.validate_checksum()?;

        let header = self.parse_header()?;
        if header.version != 2 {
            return Err(ParsingError::UnsupportedVersion(header.version));
        }

        let entries = self.parse_entries(header.number_of_entries)?;
        self.parse_extensions()?;

        let index = Index {
            entries,
            version: header.version,
        };

        if !index.is_entries_sorted() {
            return Err(ParsingError::WrongEntriesOrder);
        }
        Ok(index)
    }

    fn validate_checksum(&mut self) -> Result<(), ParsingError> {
        debug_assert!(self.data.len() >= SHA1_SIZE_IN_BYTES);
        let (data, checksum) = self.data.split_at(self.data.len() - SHA1_SIZE_IN_BYTES);

        let mut hasher = Sha1Hasher::new();
        hasher.update(data);
        if hasher.finalize().as_bytes() != checksum {
            return Err(ParsingError::InvalidCheckSum);
        }

        self.data = data;
        Ok(())
    }

    fn parse_entries(&mut self, number_of_entries: u32) -> Result<Vec<IndexEntry>, ParsingError> {
        let mut entries = Vec::with_capacity(number_of_entries as usize);
        let mut parser = EntryParser::new(&self.data);
        for i in 0..number_of_entries {
            let entry = parser
                .parse_one_entry()
                .ok_or(ParsingError::EntryParseFail(i))?;
            entries.push(entry);
        }

        self.data = parser.data();
        Ok(entries)
    }

    fn parse_header(&mut self) -> Result<IndexHeader, ParsingError> {
        debug_assert!(self.data.len() >= INDEX_HEADER_SIZE);

        let data = self.data;
        let (signature, data) = data.split_at(4);
        if signature != INDEX_SIGNATURE {
            return Err(ParsingError::InvalidSignature);
        }

        let (version, data) = data.split_at(4);
        let version = u32_from_be_slice(version);

        let (number_of_entries, data) = data.split_at(4);
        let number_of_entries = u32_from_be_slice(number_of_entries);

        self.data = data;
        Ok(IndexHeader {
            version,
            number_of_entries,
        })
    }

    fn parse_extensions(&mut self) -> Result<(), ParsingError> {
        // NOTE: All extensions unsupported
        while !self.data.is_empty() {
            let (signature, rest) = self
                .data
                .split_first_chunk::<EXTENSION_SIGNATURE_SIZE>()
                .ok_or(ParsingError::TooShort)?;

            if !is_extension_ignorable(signature) && !self.is_extension_supported(signature) {
                let signature = str::from_utf8(signature).unwrap_or("unknown").to_string();
                return Err(ParsingError::UnsupportedExtension(signature));
            }

            let (size, rest) = rest
                .split_first_chunk::<4>()
                .ok_or(ParsingError::TooShort)?;
            let size = u32::from_be_bytes(*size);
            let (_, rest) = rest
                .split_at_checked(size as usize)
                .ok_or(ParsingError::TooShort)?;
            self.data = rest;
        }

        Ok(())
    }

    fn is_extension_supported(&self, _signature: &[u8; 4]) -> bool {
        return false;
    }
}

struct EntryParser<'a> {
    data: &'a [u8],
    consumed: usize,
}

impl<'a> EntryParser<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, consumed: 0 }
    }

    fn parse_one_entry(&mut self) -> Option<IndexEntry> {
        let ctime = self.parse_time()?;
        let mtime = self.parse_time()?;
        let dev = self.parse_u32()?;
        let ino = self.parse_u32()?;
        let mode = self.parse_u32()?;
        let uid = self.parse_u32()?;
        let gid = self.parse_u32()?;
        let size = self.parse_u32()?;
        let id = self.parse_object_id()?;
        let flags = self.parse_u16()?;
        let path = self.parse_path(flags)?;

        self.skip_null_byte_and_padding();

        let entry = IndexEntry {
            mtime,
            ctime,
            dev,
            ino,
            mode,
            uid,
            gid,
            size,
            id,
            flags,
            path,
        };
        Some(entry)
    }

    fn parse_u16(&mut self) -> Option<u16> {
        let n = self.data.split_off(..2)?;
        self.consumed += n.len();
        let n = u16_from_be_slice(n);
        Some(n)
    }

    fn parse_u32(&mut self) -> Option<u32> {
        let n = self.data.split_off(..4)?;
        self.consumed += n.len();
        let n = u32_from_be_slice(n);
        Some(n)
    }

    fn parse_time(&mut self) -> Option<IndexTime> {
        let seconds = self.parse_u32()?;
        let nanoseconds = self.parse_u32()?;
        let time = IndexTime {
            seconds,
            nanoseconds,
        };
        Some(time)
    }

    fn parse_object_id(&mut self) -> Option<ObjectId> {
        let hash = self.data.split_off(..SHA1_SIZE_IN_BYTES)?;
        self.consumed += hash.len();
        ObjectId::from_bytes(hash)
    }

    fn parse_path(&mut self, flags: u16) -> Option<Vec<u8>> {
        let path_len = flags & 0xFFF;
        if path_len == 0xFFF {
            self.parse_path_name_until_null()
        } else {
            self.parse_path_name_by_len(path_len as usize)
        }
    }

    fn parse_path_name_by_len(&mut self, size: usize) -> Option<Vec<u8>> {
        let path = self.data.split_off(..size)?;
        self.consumed += path.len();
        Some(path.to_vec())
    }

    fn parse_path_name_until_null(&mut self) -> Option<Vec<u8>> {
        let i = self.data.iter().position(|c| *c == b'0')?;
        self.parse_path_name_by_len(i)
    }

    fn skip_null_byte_and_padding(&mut self) -> Option<()> {
        // --------------------------------------------------
        // | name | \0 | padding of \0 to multiple of eight |
        // --------------------------------------------------
        let n = align_padding_size(self.consumed + 1);
        let _ = self.data.split_off(..(n + 1))?;
        self.consumed += n + 1;
        Some(())
    }

    fn data(self) -> &'a [u8] {
        self.data
    }
}

fn is_extension_ignorable(signature: &[u8; 4]) -> bool {
    signature[0].is_ascii_uppercase()
}

#[derive(Debug, Clone, Copy)]
struct IndexHeader {
    version: u32,
    number_of_entries: u32,
}

fn u16_from_be_slice(b: &[u8]) -> u16 {
    debug_assert!(b.len() >= 2);
    u16::from_be_bytes(b.try_into().unwrap())
}

fn u32_from_be_slice(b: &[u8]) -> u32 {
    debug_assert!(b.len() >= 4);
    u32::from_be_bytes(b.try_into().unwrap())
}

pub fn write_index(repo: &Repository, index: &Index) -> io::Result<()> {
    let index_path = repo.git_dir().join("index");
    // TODO: File::create() truncates file if it exists.
    // Maybe use create_new() and swap files.
    let mut index_file = File::create(index_path)?;
    let mut writer = IndexWriter::new(&mut index_file);
    writer.write_index(index)
}

struct IndexWriter<W: Write> {
    writer: W,
    hasher: Sha1Hasher,
    written: usize,
}

impl<W: Write> Write for IndexWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.writer.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

impl<W: Write> IndexWriter<W> {
    pub fn new(dst: W) -> Self {
        Self {
            writer: dst,
            hasher: Sha1Hasher::new(),
            written: 0,
        }
    }

    pub fn write_index(&mut self, index: &Index) -> io::Result<()> {
        self.write_index_header(IndexHeader {
            version: index.version,
            number_of_entries: index.entries.len() as u32,
        })?;

        for entry in &index.entries {
            self.write_one_entry(entry)?;
        }

        let checksum = self.checksum();
        self.write_all(checksum.as_bytes())?;
        Ok(())
    }

    fn write_index_header(&mut self, header: IndexHeader) -> io::Result<()> {
        self.write_all(INDEX_SIGNATURE)?;
        self.write_all(&header.version.to_be_bytes())?;
        self.write_all(&header.number_of_entries.to_be_bytes())?;
        Ok(())
    }

    fn write_one_entry(&mut self, entry: &IndexEntry) -> io::Result<()> {
        debug_assert_eq!(self.written, 0);
        self.write_time(entry.ctime)?;
        self.write_time(entry.mtime)?;

        self.write_u32(entry.dev)?;
        self.write_u32(entry.ino)?;
        self.write_u32(entry.mode)?;
        self.write_u32(entry.uid)?;
        self.write_u32(entry.gid)?;
        self.write_u32(entry.size)?;

        self.write_object_id(&entry.id)?;

        self.write_u16(entry.flags)?;
        self.write_path(&entry.path)?;

        let padding = [0u8; 8];
        let n = align_padding_size(self.written);
        self.write_all(&padding[..n])?;

        self.written = 0;
        Ok(())
    }

    fn write_time(&mut self, time: IndexTime) -> io::Result<()> {
        self.write_u32(time.seconds)?;
        self.write_u32(time.nanoseconds)?;
        Ok(())
    }

    fn write_u16(&mut self, n: u16) -> io::Result<()> {
        let bytes = &n.to_be_bytes();
        self.write_all(bytes)?;
        self.written += bytes.len();
        Ok(())
    }

    fn write_u32(&mut self, n: u32) -> io::Result<()> {
        let bytes = &n.to_be_bytes();
        self.write_all(bytes)?;
        self.written += bytes.len();
        Ok(())
    }

    fn write_object_id(&mut self, hash: &ObjectId) -> io::Result<()> {
        let hash = hash.as_bytes();
        self.write_all(hash)?;
        self.written += hash.len();
        Ok(())
    }

    fn write_path(&mut self, path: &[u8]) -> io::Result<()> {
        self.write_all(path)?;
        self.write_all(b"\0")?;
        self.written += path.len() + 1;
        Ok(())
    }

    fn checksum(&mut self) -> Sha1 {
        self.hasher.finalize_reset()
    }
}

fn align_padding_size(size: usize) -> usize {
    return (8 - (size % 8)) % 8;
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateIndexError {
    #[error("stat() call failed: {0}")]
    StatFailed(io::Error),

    #[error("only regular files are supported")]
    NotRegularFile,

    #[error("file not found: {0}")]
    FileNotFound(io::Error),

    #[error("file not found: {0}")]
    WriteObjectFailed(object_db::Error),
}

pub fn add_path_to_index(
    path: &Path,
    repo: &Repository,
    index: &mut Index,
) -> Result<(), UpdateIndexError> {
    let stat = fs::metadata(path).map_err(|e| UpdateIndexError::StatFailed(e))?;

    if !stat.is_file() {
        return Err(UpdateIndexError::NotRegularFile);
    }

    let file = File::open(path).map_err(|e| UpdateIndexError::FileNotFound(e))?;
    let db = repo.object_db();
    let id = db
        .write_file(&file, &stat, ObjectType::Blob)
        .map_err(|e| UpdateIndexError::WriteObjectFailed(e))?;
    let entry = create_index_entry(path, &stat, id);
    index.add(entry);
    Ok(())
}

fn create_index_entry(path: &Path, stat: &Metadata, id: ObjectId) -> IndexEntry {
    let mtime = IndexTime {
        seconds: stat.mtime() as u32,
        nanoseconds: stat.mtime_nsec() as u32,
    };

    let ctime = IndexTime {
        seconds: stat.ctime() as u32,
        nanoseconds: stat.ctime_nsec() as u32,
    };

    let dev = stat.dev() as u32;
    let ino = stat.ino() as u32;
    let uid = stat.uid() as u32;
    let gid = stat.gid() as u32;
    let size = stat.size() as u32;
    let mode = stat_mode_to_index_mode(stat);

    let path = path.as_os_str().as_encoded_bytes().to_vec();

    let path_len = path.len();
    let flags = if path_len < 0xFFF {
        path_len as u16
    } else {
        0xFFF
    };

    IndexEntry {
        mtime,
        ctime,
        dev,
        ino,
        mode,
        uid,
        gid,
        size,
        id,
        flags,
        path,
    }
}

fn stat_mode_to_index_mode(stat: &Metadata) -> u32 {
    if stat.is_file() {
        if stat.mode() & 0o100 != 0 {
            GIT_MODE_BLOB_EXECUTABLE
        } else {
            GIT_MODE_BLOB
        }
    } else if stat.is_symlink() {
        GIT_MODE_LINK
    } else {
        debug_assert!(false, "File type: {}", stat.mode());
        stat.mode()
    }
}

pub fn remove_path_from_index(path: &Path, index: &mut Index) -> io::Result<()> {
    if let Ok(i) = index.find_by_path(path) {
        index.remove(i);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn read_write_identity() -> testing::Result<()> {
        let raw_index = get_test_raw_index();

        let mut parser = IndexParser::new(&raw_index[..]);
        let index = parser.parse_index()?;

        let mut buffer = Vec::new();
        let mut writer = IndexWriter::new(&mut buffer);
        writer.write_index(&index)?;

        assert_eq!(raw_index, &buffer[..]);
        Ok(())
    }

    fn get_test_raw_index() -> Vec<u8> {
        let signature = b"DIRC";
        let version = 2_u32;
        let entries = [IndexEntry {
            mtime: IndexTime {
                seconds: 1770570963,
                nanoseconds: 108954535,
            },
            ctime: IndexTime {
                seconds: 1770570963,
                nanoseconds: 108954535,
            },
            dev: 2096,
            ino: 107557,
            mode: 0o100644,
            uid: 1000,
            gid: 1000,
            size: 4,
            id: ObjectId::from_str("85df50785d62d3b05ab03d9cbf7e4a0b49449730").unwrap(),
            flags: 9,
            path: b"hello.txt\0".to_vec(),
        }];

        let count_entries = entries.len() as u32;
        let mut buffer = vec![];
        buffer.extend_from_slice(signature);
        buffer.extend_from_slice(&version.to_be_bytes());
        buffer.extend_from_slice(&count_entries.to_be_bytes());
        for e in entries {
            buffer.extend_from_slice(&e.mtime.seconds.to_be_bytes());
            buffer.extend_from_slice(&e.mtime.nanoseconds.to_be_bytes());
            buffer.extend_from_slice(&e.ctime.seconds.to_be_bytes());
            buffer.extend_from_slice(&e.ctime.nanoseconds.to_be_bytes());

            buffer.extend_from_slice(&e.dev.to_be_bytes());
            buffer.extend_from_slice(&e.ino.to_be_bytes());
            buffer.extend_from_slice(&e.mode.to_be_bytes());
            buffer.extend_from_slice(&e.uid.to_be_bytes());
            buffer.extend_from_slice(&e.gid.to_be_bytes());
            buffer.extend_from_slice(&e.size.to_be_bytes());

            buffer.extend_from_slice(&e.id.as_bytes());
            buffer.extend_from_slice(&e.flags.to_be_bytes());
            buffer.extend_from_slice(&e.path[..]);
        }

        buffer.extend_from_slice(
            &Sha1::from_str("e314387b487ee7cedb6c1a5463c9db5c4e61998f")
                .unwrap()
                .as_bytes(),
        );
        buffer
    }
}
