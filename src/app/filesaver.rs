// FileSaver

use std::io;
use std::path::PathBuf;
use std::time::SystemTime;
use std::collections::HashMap;
use std::fs::{self, Permissions};
use std::os::unix::fs::MetadataExt;

use is_executable::is_executable;
use lazy_static::lazy_static;
use ratatui::{
    style::{Stylize, Style},
    text::Span,
};

lazy_static! {
    /// The cache of uid -> user name, read from `/etc/passwd` once.
    static ref USER_NAMES: HashMap<u32, String> = read_user_names();
}

/// The structure used to save file information.
#[derive(Clone)]
pub struct FileSaver {
    pub name: String,
    /// The name of the user that owns this file.
    pub owner: String,
    pub is_file: bool,
    pub is_dir: bool,
    pub executable: bool,
    pub cannot_read: bool,
    pub dangling_symlink: bool,
    pub symlink_file: Option<PathBuf>,

    size: u64,
    permissions: Option<Permissions>,
    modified_time: Option<SystemTime>,
}

impl Default for FileSaver {
    fn default() -> Self {
        FileSaver {
            name: String::new(),
            owner: String::new(),
            is_file: true,
            is_dir: false,
            cannot_read: false,
            dangling_symlink: false,
            size: 0,
            executable: false,
            permissions: None,
            modified_time: None,
            symlink_file: None
        }
    }
}

impl FileSaver {
    /// FILE_PATH is the full path of file.
    pub fn new<S, P>(file_name: S,
                     file_path: P,
                     meta: Option<io::Result<fs::Metadata>>
    ) -> Self
    where S: Into<String>,
          P: AsRef<std::path::Path>
    {
        let meta = if let
            Some(metadata) = meta
        {
            metadata
        } else {
            fs::metadata(&file_path)  
        };
        match meta {
            Err(e) => {
                match e.kind() {
                    io::ErrorKind::NotFound => {
                        FileSaver::default().dangling_symlink(file_name)
                    },
                    io::ErrorKind::PermissionDenied => {
                        FileSaver::default().cannot_read(file_name)
                    },
                    _ => panic!("Cannot get metadata of file!")
                }
            },
            Ok(metadata) => {
                let is_symlink = match fs::symlink_metadata(&file_path) {
                    Ok(meta) => {
                        meta.is_symlink()
                    },
                    _ => false
                };
                let symlink_file: Option<PathBuf> = if is_symlink {
                    Some(fs::read_link(&file_path).expect("Unable to read symlink!"))
                } else {
                    None
                };
                let executable = is_executable(&file_path);

                FileSaver {
                    name: file_name.into(),
                    owner: owner_name(metadata.uid()),
                    size: metadata.len(),
                    is_file: metadata.is_file(),
                    is_dir: metadata.is_dir(),
                    dangling_symlink: false,
                    symlink_file,
                    executable,
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

    fn dangling_symlink<T: Into<String>>(self, name: T) -> Self {
        let mut temp = self;
        temp.name = name.into();
        temp.dangling_symlink = true;
        temp
    }

    fn cannot_read<T: Into<String>>(self, name: T) -> Self {
        let mut temp = self;
        temp.name = name.into();
        temp.cannot_read = true;
        temp
    }

    /// Get permission span of file.
    pub fn permission_span<'a>(&self) -> Span<'a> {
        if self.read_only() {
            Span::raw("READONLY").red().bold()
        } else {
            Span::raw("MUTABLE").light_green().bold()
        }
    }

    /// Get owner span of file.
    pub fn owner_span<'a>(&self) -> Span<'a> {
        Span::raw(self.owner.to_owned())
    }

    pub fn read_only(&self) -> bool {
        if let Some(ref permission) = self.permissions {
            permission.readonly()
        } else {
            // When DANGLING_SYMLINK is true, the file must be mutable.
            if self.dangling_symlink {
                return false
            }
            panic!("Unknow Error!")
        }
    }

    pub fn set_modified(&mut self, time: SystemTime) {
        self.modified_time = Some(time);
    }

    pub fn modified_span<'a>(&self) -> Span<'a> {
        use chrono::{DateTime, Local};

        if let Some(time) = self.modified_time {
            let datetime: DateTime<Local> = time.into();
            Span::raw(datetime.format("%Y-%m-%d %H:%M").to_string())
        } else {
            Span::raw("")
        }
    }

    pub fn size_span<'a>(&self) -> Span<'a> {
        Span::raw(file_size::fit_4(self.size))
    }

    pub fn symlink_span<'a>(&self, style: Style) -> Span<'a> {
        let link_file = if let Some(ref file) = self.symlink_file {
            format!("-> {}", file.to_string_lossy())
        } else {
            "".into()
        };

        Span::styled(link_file, style)
    }

}

/// Get the name of the user that owns a file.
///
/// The numeric uid is returned when the user cannot be found in `/etc/passwd`.
fn owner_name(uid: u32) -> String {
    match USER_NAMES.get(&uid) {
        Some(name) => name.to_owned(),
        None => uid.to_string(),
    }
}

/// Read the uid -> user name mapping from `/etc/passwd`.
fn read_user_names() -> HashMap<u32, String> {
    match fs::read_to_string("/etc/passwd") {
        Ok(content) => parse_passwd(&content),
        Err(_) => HashMap::new(),
    }
}

/// Parse the content of `/etc/passwd` into a uid -> user name mapping.
fn parse_passwd(content: &str) -> HashMap<u32, String> {
    let mut names = HashMap::new();

    for line in content.lines() {
        let mut fields = line.split(':');

        let name = match fields.next() {
            Some(name) => name,
            None => continue,
        };

        // Skip the password field.
        if fields.next().is_none() {
            continue
        }

        let uid = match fields.next().and_then(|uid| uid.parse::<u32>().ok()) {
            Some(uid) => uid,
            None => continue,
        };

        names.insert(uid, name.to_owned());
    }

    names
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_passwd() {
        let names = parse_passwd(
            "root:x:0:0:root:/root:/bin/bash\n\
             spring:x:1000:1000::/home/spring:/bin/bash\n\
             invalid line\n\
             nobody:x:not-a-uid:65534::/nonexistent:/usr/sbin/nologin\n"
        );

        assert_eq!(names.get(&0).map(String::as_str), Some("root"));
        assert_eq!(names.get(&1000).map(String::as_str), Some("spring"));
        assert_eq!(names.len(), 2);
    }
}
