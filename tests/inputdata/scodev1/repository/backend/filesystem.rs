use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use walkdir::WalkDir;

use crate::repository::backend::*;

#[derive(Clone)]
pub struct FileSystem {
    root_directory: String,
    segments_per_folder: u64,
    segment_size: u64,
}

impl FileSystem {
    /// Creates a new filesystem backend with the default number of segements per
    /// directory (250) and segment size (250MB)
    pub fn new(root_directory: &str) -> FileSystem {
        let segments_per_folder: u64 = 250;
        let segment_size: u64 = 250 * 10_u64.pow(3);
        // Create the directory if it doesn't exist
        fs::create_dir_all(root_directory).expect("Unable to create repository directory.");

        FileSystem {
            root_directory: root_directory.to_string(),
            segments_per_folder,
            segment_size,
        }
    }

    //    #[cfg(test)]
    /// Testing only constructor that has a much smaller segment size (16KB) and
    /// segements per folder (2)
    pub fn new_test(root_directory: &str) -> FileSystem {
        let segments_per_folder: u64 = 2;
        let segment_size: u64 = 16 * 10_u64.pow(3);
        // Create the directory if it doesn't exist
        fs::create_dir_all(root_directory).expect("Unable to create repository directory.");

        FileSystem {
            root_directory: root_directory.to_string(),
            segments_per_folder,
            segment_size,
        }
    }
}

impl Backend for FileSystem {
    fn get_segment(&self, id: u64) -> Option<Box<dyn Segment>> {
        let dir_name = (id / self.segments_per_folder).to_string();
        let path = Path::new(&self.root_directory)
            .join(Path::new(&dir_name))
            .join(Path::new(&id.to_string()));
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .ok()?;
        let segment = FileSystemSegment {
            file,
            max_size: self.segment_size,
        };
        Some(Box::new(segment))
    }

    fn highest_segment(&self) -> u64 {
        WalkDir::new(self.root_directory.clone())
            .into_iter()
            .filter_map(std::result::Result::ok)
            .map(|i| {
                let str = i.path().file_name().unwrap().to_str();
                str.unwrap().to_string()
            })
            .filter_map(|i| i.parse::<u64>().ok())
            .fold(0, std::cmp::max)
    }

    fn make_segment(&self) -> Option<u64> {
        let id = self.highest_segment() + 1;
        let dir_name = (id / self.segments_per_folder).to_string();
        let dir_path = Path::new(&self.root_directory).join(Path::new(&dir_name));
        // Create directory if it doesnt exist
        fs::create_dir_all(dir_path.clone()).ok()?;
        // Create file
        let path = dir_path.join(Path::new(&id.to_string()));
        fs::File::create(path).ok()?;
        Some(id)
    }

    fn get_index(&self) -> Vec<u8> {
        // Make index path
        let path = Path::new(&self.root_directory).join(Path::new("index"));
        // Check to see if the index exists, otherwise return an empty path
        if path.exists() {
            let mut buffer = Vec::new();
            let mut file = fs::File::open(path).expect("Unable to open index");
            file.read_to_end(&mut buffer).expect("Unable to read index");
            buffer
        } else {
            Vec::new()
        }
    }

    fn write_index(&self, index: &[u8]) -> Result<()> {
        let path = Path::new(&self.root_directory).join(Path::new("index"));
        let mut file = fs::File::create(path)?;
        file.write_all(index)?;
        Ok(())
    }
}

pub struct FileSystemSegment {
    file: fs::File,
    max_size: u64,
}

impl Segment for FileSystemSegment {
    fn free_bytes(&self) -> u64 {
        let file_size = self.file.metadata().unwrap().len();
        if file_size > self.max_size {
            0
        } else {
            self.max_size - file_size
        }
    }

    fn read_chunk(&mut self, start: u64, length: u64) -> Option<Vec<u8>> {
        let mut output = vec![0u8; length as usize];
        self.file.seek(SeekFrom::Start(start)).ok()?;
        self.file.read_exact(&mut output).ok()?;
        Some(output)
    }

    fn write_chunk(&mut self, chunk: &[u8]) -> Option<(u64, u64)> {
        let length = chunk.len() as u64;
        let location = self.file.seek(SeekFrom::End(1)).ok()?;
        self.file.write_all(chunk).unwrap();

        Some((location, length))
    }
}
