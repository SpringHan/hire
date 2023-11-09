// FileSaver

use std::time::SystemTime;
use std::path::PathBuf;
use std::fs::{self, Permissions};

/// The structure used to save file information.
#[derive(Debug, Clone)]
pub struct FileSaver {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub symlink_file: Option<PathBuf>,
    pub modified_time: SystemTime,
    pub permissions: Permissions
}

impl FileSaver {
    pub fn new(file: fs::DirEntry) -> FileSaver {
        let file_path = file.path();
        let meta = fs::metadata(&file_path)
            .expect("Cannot get the metadata!");
        let symlink_file: Option<PathBuf> = if meta.is_symlink() {
            Some(fs::read_link(&file_path).expect("Unable to read symlink!"))
        } else {
            None
        };

        FileSaver {
            name: file.file_name().to_string_lossy().to_string(),
            size: meta.len(),
            is_dir: file_path.is_dir(),
            symlink_file,
            modified_time: meta.modified().expect("Cannot get last modified time!"),
            permissions: meta.permissions()
        }
    }
}

pub fn sort(files: &mut Vec<FileSaver>) {
    let mut directories: Vec<FileSaver> = Vec::new();
    let mut normal_files: Vec<FileSaver> = Vec::new();
    for file in files.iter() {
        if file.is_dir {
            directories.push((*file).clone());
        } else {
            normal_files.push((*file).clone());
        }
    }
    directories.sort_by(|a, b| b.name.cmp(&a.name));
    normal_files.sort_by(|a, b| b.name.cmp(&a.name));
    directories.extend(normal_files);

    *files = directories;
}
