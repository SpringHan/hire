// App

pub mod filesaver;
pub mod special_types;
pub mod command;
pub mod color;
pub use special_types::*;

use crate::key_event::Goto;
use filesaver::{FileSaver, sort};
pub use color::{TermColors, reverse_style};

use std::error;
use std::borrow::Cow;
use std::{env, fs, io};
use std::ops::{AddAssign, SubAssign};
use std::collections::HashMap;
use std::path::{PathBuf, Path};

use std::thread;
use std::sync::{Arc, Mutex};

use ratatui::widgets::ListState;

pub struct App {
    pub path: PathBuf,
    pub hide_files: bool,
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
    pub command_expand: bool,
    pub command_scroll: Option<u16>, // Used for expanded mode.

    pub command_idx: Option<usize>,
    pub command_history: Vec<String>,

    // Search file
    pub need_to_jump: bool,
    pub searched_idx: Arc<Mutex<Vec<usize>>>,

    // ColorScheme
    pub term_colors: TermColors,

    pub computer_name: Cow<'static, str>,
    pub user_name: Cow<'static, str>
}

impl Default for App {
    fn default() -> Self {
        let current_dir = env::current_dir()
            .expect("Cannot get current directory!");
        let selected_block = if current_dir.to_string_lossy() == "/" {
            Block::Browser(true)
        } else {
            Block::Browser(false)
        };
        let host_info = get_host_info();
        let term_colors = TermColors::init();
        App {
            path: current_dir,
            selected_item: ItemIndex::default(),
            parent_files: Vec::new(),
            current_files: Vec::new(),
            child_files: Vec::new(),
            hide_files: true,
            file_content: None,
            command_idx: None,
            option_key: OptionFor::None,
            command_error: false,
            command_scroll: None,
            command_expand: false,
            command_history: Vec::new(),
            searched_idx: Arc::new(Mutex::new(Vec::new())),
            need_to_jump: false,
            selected_block,
            term_colors,
            computer_name: Cow::from(host_info.0),
            user_name: Cow::from(host_info.1),
            marked_files: HashMap::new(),
            marked_operation: FileOperation::None,
        }
    }
}

// Basic
impl App {
    /// Only get the path path of current file, without its file name.
    pub fn current_path(&self) -> PathBuf {
        if self.path.to_string_lossy() == "/" {
            let current_file = self.search_file(SearchFile::Parent).unwrap();
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
            panic!("Unknown Error!");
        }

        self.refresh_parent_item();

        // Current files
        self.init_current_files()?;
        self.refresh_current_item();

        if self.current_files.is_empty() {
            if self.path.to_string_lossy() == "/" {
                let selected_file = self.get_file_saver().unwrap();
                if !selected_file.is_dir {
                    self.set_file_content()?;
                    self.refresh_child_item();
                }
            }
            return Ok(())
        }

        // Child Files
        if self.path.to_string_lossy() != "/" {
            self.init_child_files()?;
            self.refresh_child_item();
        }
        
        Ok(())
    }

    pub fn refresh_parent_item(&mut self) {
        if let None = self.selected_item.parent_selected() {
            if self.path.to_string_lossy() == "/" {
                self.selected_item.parent_select(Some(0));
            } else {
                let parent_dir = self.path.file_name().unwrap().to_string_lossy();
                let idx = self.parent_files
                    .iter()
                    .position(|e| e.name == parent_dir).unwrap();
                self.selected_item.parent_select(Some(idx));
            }
        }
    }

    pub fn refresh_current_item(&mut self) {
        if let None = self.selected_item.current_selected() {
            if !self.current_files.is_empty() {
                self.selected_item.current_select(Some(0));
            }
        }
    }

    pub fn refresh_child_item(&mut self) {
        if !self.child_files.is_empty() {
            self.selected_item.child_select(Some(0));
            self.file_content = None;
            return ()
        }

        if self.selected_item.child_selected().is_some() {
            self.selected_item.child_select(None);
        }
    }

    pub fn refresh_select_item(&mut self) {
        self.refresh_parent_item();
        self.refresh_current_item();
        self.refresh_child_item();
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
    pub fn init_current_files(&mut self) -> io::Result<()> {
        // TODO: Rewrite the logic for changing CANNOT_READ. Make it happen in this function.
        let temp_path = self.current_path();

        let mut current_files: Vec<FileSaver> = self
            .read_files(temp_path.as_path())
            .expect("Error for read_files on a file!");

        // To aovid the situation that current_files do not be refreshed
        // when reading a empty directory.
        if self.current_files.is_empty() && current_files.is_empty() {
            return Ok(())
        }

        sort(&mut current_files);

        self.current_files = current_files;
        Ok(())
    }

    /// It's your deal to ensure CURRENT_FILES is not empty.
    pub fn init_child_files(&mut self) -> io::Result<()>
    {
        let temp_path = self.path.clone();
        let current_select = {
            // TODO: Pay attention to here.
            let file_saver = self.search_file(SearchFile::Current);
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
            self.search_file(SearchFile::Parent)
        } else {
            self.search_file(SearchFile::Current)
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

    /// Partly update the file browser block.
    /// 
    /// Update parent files & current files if the user is in root directory.
    /// Otherwise update current & child files.
    pub fn partly_update_block(&mut self) -> io::Result<()> {
        if self.path.to_string_lossy() == "/" {
            self.selected_item.parent_select(Some(0));
            self.selected_item.current_select(Some(0));
            self.init_parent_files()?;
            self.init_current_files()?;
        } else {
            self.selected_item.current_select(Some(0));
            self.selected_item.child_select(None);
            self.init_current_files()?;
            self.init_child_files()?;
        }
        self.refresh_select_item();

        Ok(())
    }

    pub fn search_file(&self, file: SearchFile) -> Option<&FileSaver> {
        match file {
            SearchFile::Parent => {
                self.parent_files.get(
                    self.selected_item.parent_selected().unwrap()
                )
            },
            SearchFile::Current => {
                let idx = self.selected_item.current_selected();
                if let Some(idx) = idx {
                    self.current_files.get(idx)
                } else {
                    None
                }
            },
            SearchFile::Child => {
                let idx = self.selected_item.child_selected();
                if let Some(idx) = idx {
                    self.child_files.get(idx)
                } else {
                    None
                }
            }
        }
    }

    /// When the previous selected file doesn't exist now, return false.
    fn select_prev_file(&mut self, list: SearchFile, file: &String) -> bool
    {
        match list {
            SearchFile::Parent => {
                let file_idx = self.parent_files
                    .iter()
                    .position(|f| f.name == *file);
                self.selected_item.parent_select(
                    if file_idx.is_some() {
                        file_idx
                    } else {
                        Some(0)
                    }
                );

                if file_idx.is_none() {
                    return false
                }
            },
            SearchFile::Current => {
                let file_idx = self.current_files
                    .iter()
                    .position(|f| f.name == *file);
                self.selected_item.current_select(
                    if file_idx.is_some() {
                        file_idx
                    } else {
                        Some(0)
                    }
                );

                if file_idx.is_none() {
                    return false
                }
            },
            SearchFile::Child => {
                let file_idx = self.child_files
                    .iter()
                    .position(|f| f.name == *file);
                self.selected_item.child_select(
                    if file_idx.is_some() {
                        file_idx
                    } else {
                        Some(0)
                    }
                );

                if file_idx.is_none() {
                    return false
                }
            }
        }

        true
    }

    /// Update all files and still tick currently selected files.
    /// 
    /// When there's a file that should be selected out from this function,
    /// use TARGET argument.
    pub fn update_with_prev_selected(&mut self,
                                     target: Option<String>
    ) -> io::Result<()>
    {
        // Initialize variables.
        let parent_file = self
            .search_file(SearchFile::Parent)
            .unwrap()
            .name
            .to_owned();
        let current_file: Option<String> = if let
            Some(file) = self.search_file(SearchFile::Current)
        {
            Some(file.name.to_owned())
        } else {
            None
        };
        let child_file = if let
            Some(file) = self.search_file(SearchFile::Child)
        {
            Some(file.name.to_owned())
        } else {
            None
        };
        let has_file_content = if self.file_content.is_some() {
            true
        } else {
            false
        };

        // NOTE: When totally refresh all the files, calling the following codes.
        self.selected_item = ItemIndex::default();
        self.file_content = None;
        self.init_all_files()?;

        // The parent file is a hidden file, there's no need to find back its child files.
        if !self.select_prev_file(
            SearchFile::Parent,
            if target.is_some() && self.path.to_string_lossy() == "/"
            {
                &target.as_ref().unwrap()
            } else {
                &parent_file                
            })
        {
            return Ok(())
        }

        if self.path.to_string_lossy() == "/" {
            if !has_file_content {
                self.init_current_files()?;
                if current_file.is_some() {
                    self.select_prev_file(SearchFile::Current, &current_file.unwrap());
                } else {
                    self.refresh_current_item();
                }

                if self.file_content.is_some() {
                    self.file_content = None;
                }
            } else {
                self.set_file_content()?;
                if !self.current_files.is_empty() {
                    self.current_files.clear();
                    self.selected_item.current_select(None);
                }
            }

            return Ok(())
        }

        // When the path is not in the root directory.
        self.init_current_files()?;
        if target.is_some() {
            self.select_prev_file(SearchFile::Current, &target.unwrap());
        } else {
            if current_file.is_none()
                || !self.select_prev_file(SearchFile::Current, &current_file.unwrap())
            {
                return Ok(())
            }
        }

        if !has_file_content {
            self.init_child_files()?;
            if child_file.is_some() {
                self.select_prev_file(SearchFile::Child, &child_file.unwrap());
            } else {
                self.refresh_child_item();
            }

            if self.file_content.is_some() {
                self.file_content = None;
            }
        } else {
            self.set_file_content()?;
            if !self.child_files.is_empty() {
                self.child_files.clear();
                self.selected_item.child_select(None);
            }
        }

        Ok(())
    }

    pub fn hide_or_show(&mut self, target: Option<String>) -> io::Result<()> {
        self.hide_files = if self.hide_files {
            false
        } else {
            true
        };

        self.update_with_prev_selected(target)?;
        Ok(())
    }
}

// File Content
impl App {
    pub fn set_file_content(&mut self) -> io::Result<()> {
        use io::{Read, ErrorKind};

        let selected_file = self.get_file_saver();
        if let Some(selected_file) = selected_file {
            let file_path = self.current_path()
                .join(&selected_file.name);
            let mut content = String::new();

            match fs::File::open(file_path) {
                Err(e) => {
                    match e.kind() {
                        ErrorKind::NotFound => (),
                        ErrorKind::PermissionDenied => {
                            content = String::from("Permission Denied");
                        },
                        _ => return Err(e)
                    }
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
                let result = dir.map(filesave_closure);
                if self.hide_files {
                    Ok(result
                       .filter(|file| !file.name.starts_with("."))
                       .collect())
                } else {
                    Ok(result.collect())
                }
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
            self.command_expand = false;
            self.command_scroll = None;
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
                ":create_symlink" => {
                    self.marked_files.clear();
                    self.marked_operation = FileOperation::None;

                    command_slices.remove(0);
                    let files = command_slices.join(" ");
                    let files: Vec<&str> = files
                        .split("->")
                        .collect();
                    command::create_symlink(
                        self,
                        [(files[0].trim(), files[1].trim())].into_iter()
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

    pub fn expand_init(&mut self) {
        self.command_expand = true;
        self.command_scroll = Some(0);
    }

    pub fn expand_scroll(&mut self, direct: Goto) {
        if let Some(ref mut scroll) = self.command_scroll {
            match direct {
                Goto::Up => {
                    if *scroll > 0 {
                        scroll.sub_assign(1);
                    }
                },
                Goto::Down => scroll.add_assign(1),
                _ => panic!("Unknown error!")
            }
        }
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
        self.need_to_jump = true;
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

        if self.need_to_jump {
            self.need_to_jump = false;
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
        self.file_content = None;
        self.child_files.clear();

        if dir.as_ref().to_string_lossy() == "/" {
            if !self.command_error {
                self.selected_block = Block::Browser(true);
            }

            self.init_parent_files()?;
            self.selected_item.parent_select(Some(0));
            self.init_current_files()?;
            self.refresh_select_item();
        } else {
            self.init_all_files()?;
            if !self.command_error {
                self.selected_block = Block::Browser(false);
            }
        }

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

#[cfg(test)]
mod test {
    use ratatui::style::{Color, Modifier, Style};

    use super::*;

    #[test]
    fn test_color_init() {
        let app = App::default();
        assert_eq!(
            app.term_colors.dir_style,
            Style::new().fg(Color::Blue).add_modifier(Modifier::BOLD)
        );
        assert_eq!(
            app.term_colors.symlink_style,
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        );
    }
}
