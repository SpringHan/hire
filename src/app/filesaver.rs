// FileSaveri

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

    size: u64,
    permissions: Permissions,
    modified_time: SystemTime,
    symlink_file: Option<PathBuf>
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

    /// Get permission span of file.
    pub fn permission_span(&self) -> Span {
        if self.permissions.readonly() {
            Span::raw("READONLY").red().bold()
        } else {
            Span::raw("MUTABLE").light_green().bold()
        }
    }

    pub fn modified_span(&self) -> Span {
        use chrono::{DateTime, Utc};

        let datetime: DateTime<Utc> = self.modified_time.into();
        Span::raw(datetime.format("%Y-%m-%d %H:%M").to_string())
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
