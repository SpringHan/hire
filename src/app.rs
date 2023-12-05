// App

pub mod filesaver;
pub mod special_types;
pub mod command;
pub use special_types::*;

use crate::key_event::Goto;
use filesaver::{FileSaver, sort};

use std::error;
use std::borrow::Cow;
use std::{env, fs, io};
use std::ops::AddAssign;
use std::collections::HashMap;
use std::path::{PathBuf, Path};

use std::thread;
use std::sync::{Arc, Mutex};

use ratatui::widgets::ListState;

pub struct App {
    pub path: PathBuf,
    pub selected_item: ItemIndex,
    pub parent_files: Vec<FileSaver>,
    pub current_files: Vec<FileSaver>,
    pub child_files: Vec<FileSaver>,

    // NOTE: When file_content is not None, child_files must be empty.
    pub file_content: Option<String>,

    // Block
    pub selected_block: Block,

    pub option_key: OptionFor,       // Use the next key as option.
    pub marked_operation: FileOperation,
    pub marked_files: HashMap<PathBuf, MarkedFiles>,

    // When command_error is true, the content in command line will be displayed in red.
    pub command_error: bool,
    pub command_idx: Option<usize>,
    pub command_history: Vec<String>,

    // Search file
    pub searched_idx: Arc<Mutex<Vec<usize>>>,

    pub computer_name: Cow<'static, str>,
    pub user_name: Cow<'static, str>
}

impl Default for App {
    fn default() -> Self {
        let current_dir = env::current_dir()
            .expect("Cannot get current directory!");
        let host_info = get_host_info();
        App {
            path: current_dir,
            selected_item: ItemIndex::default(),
            parent_files: Vec::new(),
            current_files: Vec::new(),
            child_files: Vec::new(),
            file_content: None,
            command_idx: None,
            option_key: OptionFor::None,
            command_error: false,
            command_history: Vec::new(),
            searched_idx: Arc::new(Mutex::new(Vec::new())),
            selected_block: Block::Browser(false),
            computer_name: Cow::from(host_info.0),
            user_name: Cow::from(host_info.1),
            marked_files: HashMap::new(),
            marked_operation: FileOperation::None
        }
    }
}

// Basic
impl App {
    /// Only get the path path of current file, without its file name.
    pub fn current_path(&self) -> PathBuf {
        if self.path.to_string_lossy() == "/" {
            let current_file = self.get_file_saver().unwrap();
            if current_file.is_dir {
                self.path.join(current_file.name.to_owned())
            } else {
                self.path.to_owned()
            }
        } else {
            self.path.to_owned()
        }
    }

    /// Initialize parent, current and child files.
    pub fn init_all_files(&mut self) -> io::Result<()> {
        // Parent files
        self.init_parent_files()?;

        if self.parent_files.is_empty() {
            return Ok(())
        }

        // Current files
        self.init_current_files::<&str>(None)?;
        if self.current_files.is_empty() {
            self.refresh_select_item(false);
            return Ok(())
        }

        // Child Files
        let current_selected_file = self.current_files
            .get(0)
            .unwrap()
            .clone();
        self.init_child_files(
            Some(&current_selected_file)
        )?;

        self.refresh_select_item(false);
        
        Ok(())
    }

    /// Refresh the selected item of parent dir & current file.
    /// When CHILD_KEEP is true, the child index will not be changed forcibly.
    pub fn refresh_select_item(&mut self, child_keep: bool) {
        // Parent
        if let None = self.selected_item.parent_selected() {
            let parent_dir = self.path.file_name()
                .unwrap()
                .to_string_lossy();
            let idx = self.parent_files
                .iter()
                .position(|e| e.name == parent_dir).unwrap();
            self.selected_item.parent_select(Some(idx));
        }
        
        // Current
        if let None = self.selected_item.current_selected() {
            self.selected_item.current_select(Some(0));
        }
        
        if child_keep {
            return ()
        }

        // Child
        if !self.child_files.is_empty() {
            self.selected_item.child_select(Some(0));
            self.file_content = None;
            return ()
        }

        if self.selected_item.child_selected().is_some() {
            self.selected_item.child_select(None);
        }
    }

    pub fn init_parent_files(&mut self) -> io::Result<()> {
        let temp_path = if let Some(path) = self.path.parent() {
            path.to_path_buf()
        } else {
            // Cannot get parent directory info at root dir.
            PathBuf::from("/")
        };

        let mut parent_files: Vec<FileSaver> = self.read_files(temp_path.as_path())?;
        sort(&mut parent_files);
        self.parent_files = parent_files;

        Ok(())
    }

    /// The PATH is used when the user is in root directory.
    pub fn init_current_files<T>(&mut self, path: Option<T>) -> io::Result<()>
    where T: AsRef<Path>
    {
        // TODO: Rewrite the logic for changing CANNOT_READ. Make it happen in this function.
        let temp_path = if let Some(_path) = path {
            self.path.join(_path.as_ref())
        } else {
            self.path.clone()
        };

        let mut current_files: Vec<FileSaver> = self.read_files(temp_path.as_path())?;

        // To aovid the situation that current_files do not be refreshed
        // when reading a empty directory.
        if self.current_files.is_empty() && current_files.is_empty() {
            return Ok(())
        }

        sort(&mut current_files);

        self.current_files = current_files;
        Ok(())
    }

    /// To intialize child files, CURRENT_SELECT should be Some(FileSaver)
    /// To update child files, the value should be None.
    /// It's your deal to ensure CURRENT_FILES is not empty.
    pub fn init_child_files(&mut self,
                            current_select: Option<&FileSaver>
    ) -> io::Result<()>
    {
        let temp_path = self.path.clone();
        let current_select = if let Some(file) = current_select {
            file
        } else {
            let file_saver = self.current_files.get(
                self.selected_item.current_selected()
                    .expect("Failed to initialize child files.")
            );
            if let Some(file_saver) = file_saver {
                file_saver
            } else {
                return Ok(())
            }
        };

        if current_select.is_dir {
            let mut child_files: Vec<FileSaver> = self.read_files(
                temp_path.join(&current_select.name).as_path()
            )?;
            sort(&mut child_files);

            self.child_files = child_files;
            if self.file_content.is_some() {
                self.file_content = None;
            }
        } else {
            // See the note at the definition of App structure.
            if self.file_content.is_none() {
                self.child_files.clear();
            }

            self.set_file_content()?;
        }

        Ok(())
    }

    pub fn get_directory_mut(&mut self) -> (&mut Vec<FileSaver>, &mut ListState) {
        if self.path.to_string_lossy() == "/" {
            (&mut self.parent_files, &mut self.selected_item.parent)
        } else {
            (&mut self.current_files, &mut self.selected_item.current)
        }
    }

    pub fn get_file_saver(&self) -> Option<&FileSaver> {
        if self.path.to_string_lossy() == "/" {
            self.parent_files
                .get(self.selected_item.parent_selected().unwrap())
        } else {
            if self.current_files.is_empty() {
                None
            } else {
                let current_select = self.selected_item.current_selected();
                if let Some(idx) = current_select {
                    self.current_files.get(idx)
                } else {
                    None
                }
            }
        }
    }

    pub fn get_file_saver_mut(&mut self) -> Option<&mut FileSaver> {
        if self.path.to_string_lossy() == "/" {
            Some(&mut self.parent_files[self.selected_item.parent_selected().unwrap()])
        } else {
            if self.current_files.is_empty() {
                None
            } else {
                Some(&mut self.current_files[self.selected_item.current_selected().unwrap()])
            }
        }
    }
}

// File Content
impl App {
    pub fn set_file_content(&mut self) -> io::Result<()> {
        use io::Read;

        let selected_file = self.get_file_saver();
        if let Some(selected_file) = selected_file {
            let file_path = self.current_path()
                .join(&selected_file.name);
            let mut content = String::new();

            match fs::File::open(file_path) {
                Err(e) => {
                    if e.kind() != io::ErrorKind::PermissionDenied {
                        return Err(e)
                    }
                    content = String::from("Permission Denied");
                },
                Ok(ref mut file) => {
                    if let Err(e) = file.read_to_string(&mut content) {
                        if e.kind() != io::ErrorKind::InvalidData {
                            return Err(e)
                        }
                        content = String::from("Non-UTF-8 Data");
                    }
                },
            };
            self.file_content = Some(content);
        }

        Ok(())
    }

    fn read_files(&mut self, path: &Path) -> Result<Vec<FileSaver>, io::Error> {
        let temp_dir = fs::read_dir(path);

        match temp_dir {
            Ok(dir) => {
                let result: Vec<FileSaver> = dir.map(filesave_closure).collect();
                Ok(result)
            }
            Err(err) => {
                if err.kind() == io::ErrorKind::PermissionDenied {
                    let temp_file = self.get_file_saver_mut().unwrap();
                    temp_file.cannot_read = true;
                    Ok(Vec::new())
                } else {
                    Err(err)
                }
            },
        }
    }
}

// Command Line
impl App {
    pub fn set_command_line<T: Into<String>>(&mut self, content: T, pos: CursorPos) {
        self.selected_block = Block::CommandLine(content.into(), pos);
    }

    pub fn command_line_append(&mut self, content: char) {
        if let
            Block::CommandLine(
                ref mut origin,
                ref mut cursor
            ) = self.selected_block
        {
            if let CursorPos::Index(idx) = cursor {
                origin.insert(*idx, content);
                idx.add_assign(1);
                return ()
            }

            origin.push(content);
        }
    }
    
    /// Quit command line selection.
    pub fn quit_command_mode(&mut self) {
        self.selected_block = self::Block::Browser(
            if self.path.to_string_lossy() == "/" {
                true
            } else {
                false
            }
        );

        if self.command_idx.is_some() {
            self.command_idx = None;
        }

        if self.command_error {
            self.command_error = false;
        }
    }
    
    /// The function will change content in command line.
    /// In the meanwhile, adjusting current command index.
    pub fn command_select(&mut self, direct: Goto) {
        if let
            Block::CommandLine(
                ref mut current,
                ref mut cursor
            ) = self.selected_block
        {
            if self.command_history.is_empty() {
                return ()
            }

            if *cursor != CursorPos::End {
                *cursor = CursorPos::End;
            }

            if let Some(index) = self.command_idx {
                match direct {
                    Goto::Up => {
                        if index == 0 {
                            return ()
                        }
                        self.command_idx = Some(index - 1);
                        *current = self.command_history[index - 1].to_owned();
                    },
                    Goto::Down => {
                        if index == self.command_history.len() - 1 {
                            return ()
                        }
                        self.command_idx = Some(index + 1);
                        *current = self.command_history[index + 1].to_owned()
                    },
                    _ => panic!("Unvalid value!")
                }
                return ()
            }

            // Initial selection.
            let current_idx = match direct {
                Goto::Up => {
                    self.command_history
                        .iter()
                        .rev()
                        .position(|x| x == current)
                },
                Goto::Down => {
                    self.command_history
                        .iter()
                        .position(|x| x == current)
                },
                _ => panic!("Unvalid value!")
            };
            if let Some(idx) = current_idx {
                if direct == Goto::Up {
                    // The real idx is: len - 1 - IDX
                    if idx == self.command_history.len() - 1 {
                        self.command_idx = Some(0);
                        return ()
                    }
                    let temp_idx = self.command_history.len() - 2 - idx;
                    self.command_idx = Some(temp_idx);
                    *current = self.command_history[temp_idx].to_owned();
                } else {
                    if idx + 1 == self.command_history.len() {
                        self.command_idx = Some(idx);
                        return ()
                    }
                    self.command_idx = Some(idx + 1);
                    *current = self.command_history[idx + 1].to_owned();
                }
            } else {
                if direct == Goto::Up {
                    self.command_idx = Some(self.command_history.len() - 1);
                    *current = self.command_history.last().unwrap().to_owned();
                } else {
                    self.command_idx = Some(0);
                    *current = self.command_history.first().unwrap().to_owned();
                }
            }
        }
    }
    
    pub fn command_parse(&mut self) -> io::Result<()> {
        if self.command_error {
            return Ok(self.quit_command_mode())
        }

        if let Block::CommandLine(ref command, _) = self.selected_block {
            if command.starts_with("/") {
                self.file_search(command[1..].to_owned());
                return Ok(self.quit_command_mode())
            }

            self.command_history.push(command.to_owned());
            let mut command_slices: Vec<&str> = command.split(" ").collect();
            let ready_for_check = match command_slices[0] {
                ":rename" => {
                    command_slices.remove(0);
                    let file_name = command_slices.join(" ");
                    command::rename_file(
                        self.path.to_owned(),
                        self,
                        file_name
                    )?
                },
                ":create_file" => {
                    command_slices.remove(0);
                    let files = command_slices.join(" ");
                    let files: Vec<&str> = files.split(",").collect();
                    command::create_file(
                        self,
                        files.into_iter(),
                        false
                    )?
                },
                ":create_dir" => {
                    command_slices.remove(0);
                    let files = command_slices.join(" ");
                    let files: Vec<&str> = files.split(",").collect();
                    command::create_file(
                        self,
                        files.into_iter(),
                        true
                    )?
                },
                _ => command::OperationError::UnvalidCommand
            };

            // When result of check is false, there would be errors, which should be displayed.
            if ready_for_check.check(self) {
                self.quit_command_mode();
            }
        }

        Ok(())
    }
}

// File Search
impl App {
    pub fn file_search(&mut self, name: String) {
        let idx = Arc::clone(&self.searched_idx);
        if !idx.lock().unwrap().is_empty() {
            idx.lock().unwrap().clear();
        }

        self.command_history.push(format!("/{}", name.clone()));
        // Use this way as we cannot change the selected_block at the same time.
        let current_files = self.get_directory_mut().0.clone();

        thread::spawn(move || {
            let mut i = 0;
            let name = name.to_lowercase();
            for file in current_files.iter() {
                if file.name.to_lowercase().contains(&name) {
                    idx.lock().unwrap().push(i);
                }
                i += 1;
            }
        });
    }

    pub fn prev_candidate(&mut self) -> Result<(), Box<dyn error::Error>> {
        self.move_candidate(false)?;
        Ok(())
    }

    pub fn next_candidate(&mut self) -> Result<(), Box<dyn error::Error>> {
        self.move_candidate(true)?;
        Ok(())
    }

    /// Move current cursor to next/previous searched file name.
    /// When NEXT is true, searching the next. Otherwise the previous.
    fn move_candidate(&mut self,
                      next: bool
    ) -> Result<(), Box<dyn error::Error>> {
        use crate::key_event::move_cursor;

        let candidates = Arc::clone(&self.searched_idx);
        if candidates.lock().unwrap().is_empty() {
            return Ok(())
        }

        let in_root = if let Block::Browser(true) = self.selected_block {
            true
        } else {
            false
        };
        let current_idx = if in_root {
            self.selected_item.parent.selected().unwrap()
        } else {
            self.selected_item.current.selected().unwrap()
        };

        let target = if next {
            get_search_index(
                candidates.lock().unwrap().iter(),
                current_idx,
                true
            )
        } else {
            get_search_index(
                candidates.lock().unwrap().iter().rev(),
                current_idx,
                false
            )
        };

        if let Some(idx) = target {
            move_cursor(self, Goto::Index(idx), in_root)?;
        }

        Ok(())
    }
    
    pub fn clean_search_idx(&mut self) {
        self.searched_idx.lock().unwrap().clear();
    }
}

// Other Action
impl App {
    pub fn goto_dir<P: AsRef<Path>>(&mut self, dir: P) -> io::Result<()> {
        self.path = PathBuf::from(dir.as_ref());
        self.selected_item = ItemIndex::default();

        if dir.as_ref().to_string_lossy() == "/" {
            self.file_content = None;
            self.child_files.clear();
            self.selected_block = Block::Browser(true);

            self.init_parent_files()?;
            self.selected_item.parent_select(Some(0));
            self.init_current_files(Some(self.parent_files[0].name.to_owned()))?;
        } else {
            self.init_all_files()?;
            self.selected_block = Block::Browser(false);
        }
        self.refresh_select_item(false);

        Ok(())
    }
    
    /// Append FILE to marked file list.
    pub fn append_marked_file<S: Into<String>>(&mut self, file: S, is_dir: bool) {
        let path = self.current_path();

        self.marked_files
            .entry(path)
            .or_insert(MarkedFiles::default())
            .files
            .insert(file.into(), is_dir);
    }

    pub fn append_marked_files<I>(&mut self, iter: I)
    where I: Iterator<Item = FileSaver>
    {
        let path = self.current_path();

        let temp_set = self.marked_files
            .entry(path)
            .or_insert(MarkedFiles::default());
        for file in iter {
            temp_set.files.insert(file.name, file.is_dir);
        }
    }

    pub fn marked_file_contains<S: Into<String>>(&self, file: S) -> bool {
        let path = self.current_path();
        if let Some(marked_files) = self.marked_files.get(&path) {
            // In Linux, there could not be more than one files that have the same name.
            // (Include directories)
            return marked_files.files.contains_key(&file.into())
        }

        false
    }

    pub fn remove_marked_file<S: Into<String>>(&mut self, file: S) {
        let path = self.current_path();
        if let Some(marked_files) = self.marked_files.get_mut(&path) {
            marked_files.files.remove(&file.into());

            if marked_files.files.is_empty() {
                self.marked_files.remove(&path);
            }
        }
    }

    pub fn marked_file_contains_path(&self) -> bool {
        let path = self.current_path();
        self.marked_files.contains_key(&path)
    }
    
    /// Clear marked files in current directory.
    pub fn clear_path_marked_files(&mut self) {
        let path = self.current_path();
        self.marked_files.remove(&path);
    }
}

#[inline]
fn filesave_closure(ele: Result<fs::DirEntry, io::Error>) -> FileSaver {
    match ele {
        Ok(x) => FileSaver::new(
            x.file_name().to_string_lossy(),
            x.path(),
            None
        ),
        Err(_) => panic!("Cannot get a file with error!")
    }
}

// TODO: Delete commented code lines when the time is right.
#[inline]
fn get_search_index<'a, T>(iter: T,
                           current: usize,
                           next: bool
) -> Option<usize>
where T: Iterator<Item = &'a usize>
{
    // let mut prev_idx: Option<usize> = None;
    let mut get_current_idx = false;

    for i in iter {
        if get_current_idx {
            return Some(*i)
        }

        if !next && *i < current {
            return Some(*i)
        }

        if next && *i > current {
            return Some(*i)
            // if prev_idx.is_some() {
            //     return prev_idx
            // } else {
            //     return Some(*i)
            // }
        }

        if *i == current {
            // if next {
            //     get_current_idx = true;
            // } else {
            //     if prev_idx.is_some() {
            //         return prev_idx
            //     }
            //     break;
            // }
            get_current_idx = true;
            continue;
        }

        // prev_idx = Some(*i);
    }

    None
}

fn get_host_info() -> (String, String) {
    use std::process::Command;
    let host_name = unsafe {
        String::from_utf8_unchecked(
            Command::new("hostname").output().unwrap().stdout
        )
    };

    let user_name = unsafe {
        String::from_utf8_unchecked(
            Command::new("whoami").output().unwrap().stdout
        )
    };

    (host_name, user_name)
}
