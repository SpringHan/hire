// FileSaveri

use std::io;
use std::time::SystemTime;
use std::path::PathBuf;
use std::fs::{self, Permissions};
use ratatui::{
    text::Span,
    style::Stylize
};

/// The structure used to save file information.
#[derive(Debug, Clone)]
pub struct FileSaver {
    pub name: String,
    pub is_dir: bool,
    pub cannot_read: bool,
    pub dangling_symlink: bool,

    size: u64,
    permissions: Option<Permissions>,
    modified_time: Option<SystemTime>,
    symlink_file: Option<PathBuf>
}

impl Default for FileSaver {
    fn default() -> Self {
        FileSaver {
            name: String::new(),
            is_dir: false,
            cannot_read: false,
            dangling_symlink: false,
            size: 0,
            permissions: None,
            modified_time: None,
            symlink_file: None
        }
    }
}

impl FileSaver {
    pub fn new(file: fs::DirEntry) -> Self {
        let file_path = file.path();
        match fs::metadata(&file_path) {
            Err(e) => {
                match e.kind() {
                    io::ErrorKind::NotFound => {
                        FileSaver::default().dangling_symlink(
                            file.file_name().to_string_lossy()
                        )
                    },
                    io::ErrorKind::PermissionDenied => {
                        FileSaver::default().cannot_read(
                            file.file_name().to_string_lossy()
                        )
                    },
                    _ => panic!("Cannot get metadata of file!")
                }
            },
            Ok(metadata) => {
                let symlink_file: Option<PathBuf> = if metadata.is_symlink() {
                    Some(fs::read_link(&file_path).expect("Unable to read symlink!"))
                } else {
                    None
                };

                FileSaver {
                    name: file.file_name().to_string_lossy().to_string(),
                    size: metadata.len(),
                    is_dir: file_path.is_dir(),
                    dangling_symlink: false,
                    symlink_file,
                    cannot_read: false,
                    modified_time: Some(
                        metadata.modified()
                            .expect("Cannot get last modified time!")
                    ),
                    permissions: Some(metadata.permissions())
                }

            }
        }
    }

    fn dangling_symlink<T: ToString>(self, name: T) -> Self {
        let mut temp = self;
        temp.name = name.to_string();
        temp.dangling_symlink = true;
        temp
    }

    fn cannot_read<T: ToString>(self, name: T) -> Self {
        let mut temp = self;
        temp.name = name.to_string();
        temp.cannot_read = true;
        temp
    }

    /// Get permission span of file.
    pub fn permission_span(&self) -> Span {
        if let Some(ref permission) = self.permissions {
            if permission.readonly() {
                Span::raw("READONLY").red().bold()
            } else {
                Span::raw("MUTABLE").light_green().bold()
            }
        } else {
            panic!("Unknown Error!")
        }
    }

    pub fn modified_span(&self) -> Span {
        use chrono::{DateTime, Utc};

        if let Some(time) = self.modified_time {
            let datetime: DateTime<Utc> = time.into();
            Span::raw(datetime.format("%Y-%m-%d %H:%M").to_string())
        } else {
            Span::raw("")
        }
    }

    // TODO: Check the result
    pub fn size_span(&self) -> Span {
        Span::raw(file_size::fit_4(self.size))
    }

    pub fn symlink_span(&self) -> Span {
        let link_file = if let Some(ref file) = self.symlink_file {
            file.to_string_lossy()
        } else {
            "".into()
        };

        Span::raw(link_file).light_cyan()
    }

}

pub fn sort(files: &mut Vec<FileSaver>) {
    if files.is_empty() {
        return ()
    }

    let mut directories: Vec<FileSaver> = Vec::new();
    let mut normal_files: Vec<FileSaver> = Vec::new();
    for file in files.iter() {
        if file.is_dir {
            directories.push((*file).clone());
        } else {
            normal_files.push((*file).clone());
        }
    }
    directories.sort_by(|a, b| a.name.cmp(&b.name));
    normal_files.sort_by(|a, b| a.name.cmp(&b.name));
    directories.extend(normal_files);

    *files = directories;
}
