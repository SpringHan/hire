//! The metadata behind a directory entry and its `ls -l`-style state line.

use std::io;
use std::path::PathBuf;
use std::time::SystemTime;
use std::collections::HashMap;
use std::fs::{self, Permissions};
use std::os::unix::fs::{MetadataExt, PermissionsExt};

use is_executable::is_executable;
use lazy_static::lazy_static;
use ratatui::{
    style::{Color, Style},
    text::Span,
};

lazy_static! {
    /// The cache of uid -> user name, read from `/etc/passwd` once.
    static ref USER_NAMES: HashMap<u32, String> = read_user_names();
}

/// The cached metadata of one directory entry.
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
    /// Build an entry; `meta` skips a second `stat` when the caller has it.
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

    /// The `ls -l` style mode text, e.g. `-rw-r--r--`.
    ///
    /// A symlink shows the mode of its target, like the rest of the state
    /// line. The string is empty only without metadata (`DENIED` /
    /// `DANGLING_SYMLINK` entries, rendered differently); it is the text
    /// behind [`FileSaver::mode_spans`].
    fn mode_string(&self) -> String {
        let Some(permissions) = self.permissions.as_ref() else {
            return String::new()
        };

        let mode = permissions.mode();

        let kind = match mode & libc::S_IFMT {
            libc::S_IFDIR => 'd',
            libc::S_IFCHR => 'c',
            libc::S_IFBLK => 'b',
            libc::S_IFIFO => 'p',
            libc::S_IFSOCK => 's',
            _ => '-',
        };

        // One entry per permission triad: where it starts, the special bit it
        // carries and its letter (`s` for setuid/setgid, `t` for the sticky
        // bit). The letter is upper case when the execute bit is clear.
        let triads = [
            (6, libc::S_ISUID, 's'),
            (3, libc::S_ISGID, 's'),
            (0, libc::S_ISVTX, 't'),
        ];

        let mut string = String::with_capacity(10);
        string.push(kind);

        for (shift, special, special_char) in triads {
            let bits = (mode >> shift) & 0o7;
            let special_set = mode & special != 0;

            string.push(if bits & 0o4 != 0 { 'r' } else { '-' });
            string.push(if bits & 0o2 != 0 { 'w' } else { '-' });

            string.push(match (special_set, bits & 0o1 != 0) {
                (false, true) => 'x',
                (true, true) => special_char,
                (true, false) => special_char.to_ascii_uppercase(),
                (false, false) => '-',
            });
        }

        string
    }

    /// The mode text as spans, one per character, each in its own colour.
    ///
    /// Colour (see [`mode_char_style`]) is what tells the type character, the
    /// triads and the special bits apart, so the state line reads at a glance.
    pub fn mode_spans(&self) -> Vec<Span<'static>> {
        self.mode_string()
            .chars()
            .map(|character| {
                Span::styled(character.to_string(), mode_char_style(character))
            })
            .collect()
    }

    pub fn owner_span<'a>(&self) -> Span<'a> {
        Span::raw(self.owner.to_owned())
    }

    pub fn read_only(&self) -> bool {
        if let Some(ref permission) = self.permissions {
            permission.readonly()
        } else {
            // `read_only` function always should be written after `cannot_read`,
            // The 'if' is to avoid unnecessary panic.
            if self.dangling_symlink || self.cannot_read {
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

/// The colour of one character of the mode string.
///
/// The palette mirrors `app/color.rs` (`90`/`92`/`93`/`91`/`94`/`36`): the
/// special bits are cyan, the socket type character included, so no character
/// which means "special" is green.
fn mode_char_style(character: char) -> Style {
    let color = match character {
        '-' => Color::DarkGray,
        'r' => Color::LightYellow,
        'w' => Color::LightRed,
        'x' | 's' | 'S' | 't' | 'T' => Color::Cyan,
        _ => Color::LightGreen,
    };

    Style::new().fg(color)
}

/// The name of the user who owns a file.
///
/// The numeric uid is used when `/etc/passwd` has no entry for it.
fn owner_name(uid: u32) -> String {
    match USER_NAMES.get(&uid) {
        Some(name) => name.to_owned(),
        None => uid.to_string(),
    }
}

fn read_user_names() -> HashMap<u32, String> {
    match fs::read_to_string("/etc/passwd") {
        Ok(content) => parse_passwd(&content),
        Err(_) => HashMap::new(),
    }
}

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
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn test_mode_string() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir().join(format!(
            "hire-mode-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();

        let file = dir.join("file");
        fs::write(&file, "").unwrap();

        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        assert_eq!(
            FileSaver::new("file", &file, None).mode_string(),
            "-rw-r-----",
        );

        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            FileSaver::new("file", &file, None).mode_string(),
            "-rwxr-xr-x",
        );

        // A special bit replaces the execute character of its triad, and is
        // upper case when the execute bit is clear.
        fs::set_permissions(&file, fs::Permissions::from_mode(0o4755)).unwrap();
        assert_eq!(
            FileSaver::new("file", &file, None).mode_string(),
            "-rwsr-xr-x",
        );

        fs::set_permissions(&file, fs::Permissions::from_mode(0o4644)).unwrap();
        assert_eq!(
            FileSaver::new("file", &file, None).mode_string(),
            "-rwSr--r--",
        );

        // A directory carries `d`; the sticky bit is the last character.
        let sub = dir.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::set_permissions(&sub, fs::Permissions::from_mode(0o1750)).unwrap();
        assert_eq!(
            FileSaver::new("sub", &sub, None).mode_string(),
            "drwxr-x--T",
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_mode_spans_colour_each_character() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir().join(format!(
            "hire-mode-colour-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();

        // A setuid executable: `-rwsr-xr-x`.
        let file = dir.join("tool");
        fs::write(&file, "").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o4755)).unwrap();

        let spans = FileSaver::new("tool", &file, None).mode_spans();

        let text: String = spans.iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(text, "-rwsr-xr-x");

        let colours: Vec<Option<Color>> = spans.iter()
            .map(|span| span.style.fg)
            .collect();
        assert_eq!(
            colours,
            vec![
                Some(Color::DarkGray),    // -
                Some(Color::LightYellow), // r
                Some(Color::LightRed),    // w
                Some(Color::Cyan),        // s (setuid)
                Some(Color::LightYellow), // r
                Some(Color::DarkGray),    // -
                Some(Color::Cyan),        // x
                Some(Color::LightYellow), // r
                Some(Color::DarkGray),    // -
                Some(Color::Cyan),        // x
            ],
        );

        // A directory with the sticky bit: `drwxr-x--T`.
        let sub = dir.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::set_permissions(&sub, fs::Permissions::from_mode(0o1750)).unwrap();

        let spans = FileSaver::new("sub", &sub, None).mode_spans();

        let text: String = spans.iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(text, "drwxr-x--T");
        assert_eq!(spans[0].style.fg, Some(Color::LightGreen));
        assert_eq!(spans[9].style.fg, Some(Color::Cyan));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_mode_char_colours() {
        let colour = |character: char| mode_char_style(character).fg;

        for kind in ['d', 'c', 'b', 'p'] {
            assert_eq!(colour(kind), Some(Color::LightGreen), "{kind}");
        }

        assert_eq!(colour('-'), Some(Color::DarkGray));
        assert_eq!(colour('r'), Some(Color::LightYellow));
        assert_eq!(colour('w'), Some(Color::LightRed));
        assert_eq!(colour('x'), Some(Color::Cyan));

        for special in ['s', 'S', 't', 'T'] {
            assert_eq!(colour(special), Some(Color::Cyan), "{special}");
        }
    }

    #[test]
    fn test_mode_string_without_metadata() {
        let denied = FileSaver {
            cannot_read: true,
            ..FileSaver::default()
        };
        assert_eq!(denied.mode_string(), "");

        let dangling = FileSaver {
            dangling_symlink: true,
            ..FileSaver::default()
        };
        assert_eq!(dangling.mode_string(), "");
    }

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
