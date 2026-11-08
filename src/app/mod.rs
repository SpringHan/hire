// App

mod color;
mod filesaver;
mod macro_utils;
mod image_preview;

use std::{env, fs, io};
use std::time::Instant;

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{PathBuf, Path};

use ratatui::layout::Rect;
use ratatui::text::Text;
use image_preview::ImagePreview;
use ratatui::widgets::ListState;

use crate::config::{AppConfig, Keymap};
use crate::error::{AppError, AppResult};
use crate::key_event::{
    AppCompletion, EditMode, FileSearcher, NaviIndex, ShellPrefix, ShellSessions,
    SwitchCase, TabId,
};
use crate::utils::{
    read_to_text,
    file_type_label,
    MarkedFiles,
    FileContent,
    SearchFile,
    ItemIndex,
    ShellId,
    Block,
};

pub use macro_utils::*;
pub use color::TermColors;
pub use filesaver::{sort, FileSaver};
pub use image_preview::Preview;

pub struct App<'a> {
    // Core
    pub path: PathBuf,
    pub quit_now: bool,
    pub hide_files: bool,
    pub selected_item: ItemIndex,
    pub child_files: Vec<FileSaver>,
    pub parent_files: Vec<FileSaver>,
    pub current_files: Vec<FileSaver>,

    // NOTE: When file_content is not None, child_files must be empty.
    pub file_content: FileContent,

    /// Whether to show the index of file to allow user to jump to.
    pub navi_index: NaviIndex,

    // Block
    pub selected_block: Block,

    pub switch_case: Option<SwitchCase>,
    pub marked_files: HashMap<PathBuf, MarkedFiles>,

    /// When command_error is true, the content in command line will be displayed in red.
    pub command_error: bool,
    pub command_expand: bool,
    pub command_scroll: Option<(u16, u16)>, // Used for expanded mode.

    pub command_idx: Option<usize>,
    pub command_history: Vec<String>,

    pub command_completion: AppCompletion<'a>,

    // Search file
    pub file_searcher: FileSearcher,

    // Macro
    pub macro_attri: Macro,

    // ColorScheme
    pub term_colors: TermColors,

    // Target directories
    pub target_dir: HashMap<char, String>,

    /// The paths jumped from, the oldest one is stored at the first place.
    /// The length of it will never be more than 30.
    pub path_history: Vec<PathBuf>,

    // Working directory
    /// The working directory stored in the cache file, if any.
    pub working_directory: Option<PathBuf>,

    // Tab
    pub tab_list: crate::key_event::TabList<'a>,

    // Shell sessions
    pub shell_sessions: ShellSessions,

    /// The chord which gives hire a command while a shell is attached.
    pub shell_prefix: ShellPrefix,

    /// The prefix chord has been pressed; the next key is a shell command.
    pub shell_prefix_active: bool,

    /// The attached shell's scrollback owns the keys: the configured
    /// `shell_scroll_mode` bindings scroll instead of being forwarded.
    pub shell_scroll_mode: bool,

    /// A kill confirmation is waiting for `y`.
    pub shell_pending_kill: Option<ShellId>,

    /// A confirmation to kill every *other* shell of the attached shell's tab
    /// is waiting for `y`; the value is the shell which is kept.
    pub shell_pending_kill_others: Option<ShellId>,

    /// A confirmation to kill every shell of this tab is waiting for `y`.
    ///
    /// The tab id is remembered instead of a boolean so the shells of the tab
    /// the confirmation was opened for are killed even when the view changed
    /// in the meantime.
    pub shell_pending_kill_all: Option<TabId>,

    /// The shells attached most recently, newest first.
    ///
    /// `attach_active` (`B`) uses it to borrow the last used shell of another
    /// tab when the current tab has none.
    pub shell_recent: Vec<ShellId>,

    /// The shell pinned with `C-s *` (one at a time).
    ///
    /// `C-s b` in a shell view and the `attach_pinned` command in the browser
    /// attach it from anywhere, borrowing its tab when it belongs to another
    /// one. The pin is cleared when the session goes away.
    pub pinned_shell: Option<ShellId>,

    /// The shell the shell menu was opened from, if any (`C-s t`).
    ///
    /// The menu replaces the view, so closing it without attaching anything
    /// returns to this shell instead of dropping the user into the browser.
    pub shell_menu_origin: Option<ShellId>,

    /// The tab to go back to after detaching a shell borrowed from another tab.
    ///
    /// Attaching a shell of another tab switches the current tab for the
    /// duration of the view (the shell always belongs to the tab it is
    /// rendered in); this remembers the tab the user was in so `detach` can
    /// return there.
    pub shell_return_tab: Option<TabId>,

    /// Digits typed after the prefix, used to select a shell by number.
    pub shell_digits: String,

    /// When the typed digits are committed automatically.
    pub shell_digit_deadline: Option<Instant>,

    /// The whole terminal must be repainted on the next draw (after attaching
    /// or detaching a shell).
    pub clear_screen: bool,

    /// An error shown in the shell status line instead of replacing the shell
    /// view with the command line.
    pub shell_notice: Option<String>,

    /// The mouse protocol currently enabled on the *outer* terminal.
    pub outer_mouse_mode: vt100::MouseProtocolMode,

    /// Whether the outer terminal is in bracketed-paste mode.
    pub outer_bracketed_paste: bool,

    /// Buttons currently held down and forwarded to the shell.
    pub mouse_buttons: u8,

    /// The rectangle occupied by the shell screen in the last frame.
    pub shell_area: Rect,

    // Image Preview
    pub image_preview: ImagePreview,

    // Edit Mode
    pub edit_mode: EditMode,

    /// When this var is true, the navigation will be with selection.
    pub mark_expand: bool,

    // App Config
    pub keymap: Keymap,
    pub config_path: String,
    pub config: AppConfig<'a>,

    // AppErrors
    pub app_error: AppError,

    // Output
    pub quit_after_output: bool,
    /// The temporary file for communicating between hire and other programs.
    pub temp_file: String,

    // Computer & User name
    pub user_name: Cow<'static, str>,
    pub computer_name: Cow<'static, str>,
}

impl<'a> Default for App<'a> {
    fn default() -> Self {
        let current_dir = env::current_dir()
            .expect("Cannot get current directory!");
        let selected_block = if current_dir.to_string_lossy() == "/" {
            Block::Browser(true)
        } else {
            Block::Browser(false)
        };
        let tab_list = crate::key_event::TabList::new(
            current_dir.to_owned()
        );
        let host_info = get_host_info();
        let term_colors = TermColors::init();

        App {
            // Base
            path: current_dir,
            child_files: Vec::new(),
            parent_files: Vec::new(),
            current_files: Vec::new(),

            // UI
            term_colors,
            selected_block,
            hide_files: true,
            file_content: FileContent::None,
            selected_item: ItemIndex::default(),

            // Operations
            tab_list,
            shell_sessions: ShellSessions::default(),
            shell_prefix: ShellPrefix::default(),
            shell_prefix_active: false,
            shell_scroll_mode: false,
            shell_pending_kill: None,
            shell_pending_kill_others: None,
            shell_pending_kill_all: None,
            shell_recent: Vec::new(),
            pinned_shell: None,
            shell_menu_origin: None,
            shell_return_tab: None,
            shell_digits: String::new(),
            shell_digit_deadline: None,
            clear_screen: false,
            shell_notice: None,
            outer_mouse_mode: vt100::MouseProtocolMode::None,
            outer_bracketed_paste: false,
            mouse_buttons: 0,
            shell_area: Rect::default(),
            switch_case: None,
            mark_expand: false,
            command_scroll: None,
            path_history: Vec::new(),
            working_directory: None,
            target_dir: HashMap::new(),
            marked_files: HashMap::new(),
            edit_mode: EditMode::default(),
            navi_index: NaviIndex::default(),
            image_preview: ImagePreview::default(),
            file_searcher: FileSearcher::default(),

            // Command
            command_idx: None,
            command_expand: false,
            command_history: Vec::new(),
            command_completion: AppCompletion::default(),

            // Error handle
            command_error: false,
            app_error: AppError::new(),

            // Output
            quit_after_output: false,
            temp_file: String::from("/tmp/hire.tmp"),

            // Config & others
            quit_now: false,
            config: Vec::new(),
            macro_attri: Macro::new(),
            keymap: Keymap::default(),
            config_path: String::new(),
            user_name: Cow::from(host_info.1),
            computer_name: Cow::from(host_info.0),
        }
    }
}

// Basic
impl<'a> App<'a> {
    pub fn root(&self) -> bool {
        self.path.to_string_lossy() == "/"
    }

    /// When the user is in non-root dir, only return the path of current file.
    /// When the user is in root dir, return the selected file in parent block.
    pub fn current_path(&self) -> PathBuf {
        if self.root() {
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
    pub fn init_all_files(&mut self) -> AppResult<()> {
        // Parent files
        self.init_parent_files()?;

        if self.parent_files.is_empty() {
            panic!("Error at init_all_files in app.rs.");
        }

        self.refresh_parent_item();

        // Current files
        self.init_current_files()?;
        self.refresh_current_item();

        if self.current_files.is_empty() {
            if self.root() {
                let selected_file = self.get_file_saver().unwrap();
                if !selected_file.is_dir {
                    self.set_file_content()?;
                    self.refresh_child_item();
                }
            }
            return Ok(())
        }

        // Child Files
        if !self.root() {
            self.init_child_files()?;
            self.refresh_child_item();
        }
        
        Ok(())
    }

    pub fn refresh_parent_item(&mut self) {
        if let None = self.selected_item.parent_selected() {
            if self.root() {
                self.selected_item.parent_select(Some(0));
            } else {
                let parent_dir = self.path
                    .file_name()
                    .unwrap()
                    .to_string_lossy();
                let idx = self.parent_files
                    .iter()
                    .position(|e| e.name == parent_dir)
                    .expect("Error at refresh_parent_item in app.rs.");
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

    /// Select the first item if it exists, otherwise select None.
    pub fn refresh_child_item(&mut self) {
        if !self.child_files.is_empty() {
            self.selected_item.child_select(Some(0));
            self.file_content.reset();
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

    /// Select the first item or not select.
    pub fn select_normal(&mut self, file: SearchFile) {
        match file {
            SearchFile::Parent => {
                self.selected_item.parent_select(
                    if self.parent_files.is_empty() {
                        None
                    } else {
                        Some(0)
                    }
                );
            },
            SearchFile::Current => {
                self.selected_item.current_select(
                    if self.current_files.is_empty() {
                        None
                    } else {
                        Some(0)
                    }
                );
            },
            SearchFile::Child => {
                self.selected_item.child_select(
                    if self.child_files.is_empty() {
                        None
                    } else {
                        Some(0)
                    }
                );
            },
        }
    }
    
    pub fn init_parent_files(&mut self) -> io::Result<()> {
        loop {
            let temp_path = if let Some(path) = self.path.parent() {
                path.to_path_buf()
            } else {
                // Cannot get parent directory info at root dir.
                PathBuf::from("/")
            };

            let mut parent_files = self.read_files(temp_path.as_path())?;

            if temp_path.to_string_lossy() == "/" {
                sort(&mut parent_files);
                self.parent_files = parent_files;
                break;
            }

            // Handle the case when the current parent dir is not in parent_files.
            let current_parent_name = self.path
                .file_name()
                .unwrap()
                .to_string_lossy();
            if parent_files.iter()
                .position(|f| f.name == current_parent_name)
                .is_none()
            {
                match fs::metadata(self.path.to_owned()) {
                    Ok(metadata) if metadata.is_dir() => {
                        parent_files.push(
                            FileSaver::new(
                                current_parent_name,
                                self.path.parent()
                                    .expect("Error at init_parent_files in app.rs."),
                                Some(Ok(metadata))
                            )
                        );

                        sort(&mut parent_files);
                    },
                    _ => {
                        sort(&mut parent_files);

                        match parent_files.get(0) {
                            Some(file) if file.is_dir => {
                                self.path.set_file_name(file.name.to_owned());
                            },
                            _ => {
                                self.path.pop();
                                continue;
                            },
                        }
                    },
                }
            } else {
                sort(&mut parent_files);
            }


            self.parent_files = parent_files;
            break;
        }

        Ok(())
    }

    /// The PATH is used when the user is in root directory.
    pub fn init_current_files(&mut self) -> io::Result<()> {
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
    pub fn init_child_files(&mut self) -> AppResult<()>
    {
        let temp_path = self.path.clone();
        let current_select = {
            // NOTE: Pay attention to here (An incomplete NOTE, to be removed later).
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
                self.file_content.reset();
            }
        } else {
            // See the note at the definition of App structure.
            if !self.file_content.is_some() {
                self.child_files.clear();
            }

            self.set_file_content()?;
        }

        Ok(())
    }

    pub fn get_directory_mut(&mut self) -> (&mut Vec<FileSaver>, &mut ListState) {
        if self.root() {
            (&mut self.parent_files, &mut self.selected_item.parent)
        } else {
            (&mut self.current_files, &mut self.selected_item.current)
        }
    }

    pub fn get_file_saver(&self) -> Option<&FileSaver> {
        if self.root() {
            self.search_file(SearchFile::Parent)
        } else {
            self.search_file(SearchFile::Current)
        }
    }

    pub fn get_file_saver_mut(&mut self) -> Option<&mut FileSaver> {
        if self.root() {
            Some(&mut self.parent_files[
                self.selected_item.parent_selected().unwrap()
            ])
        } else {
            if self.current_files.is_empty() {
                None
            } else {
                Some(&mut self.current_files[
                    self.selected_item.current_selected().unwrap()
                ])
            }
        }
    }

    /// Partly update the file browser block.
    /// 
    /// Update parent files & current files if the user is in root directory.
    /// Otherwise update current & child files.
    pub fn partly_update_block(&mut self) -> AppResult<()> {
        if self.root() {
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

    /// Update all files and still tick currently selected files.
    /// 
    /// When there's a file that should be selected out from this function,
    /// use TARGET argument.
    pub fn update_with_prev_selected(&mut self, target: Option<String>) -> AppResult<()> {
        // Prev states
        let parent_file = self
            .search_file(SearchFile::Parent)
            .unwrap()
            .name
            .to_owned();

        let current_file = if
            let Some(file) = self.search_file(SearchFile::Current)
        {
            Some(file.name.to_owned())
        } else {
            None
        };

        let child_file = if
            let Some(file) = self.search_file(SearchFile::Child)
        {
            Some(file.name.to_owned())
        } else {
            None
        };

        // The first one is for parent, the other one for current.
        let mut select_prev = [true, true];
        let root = self.root();

        self.selected_item = ItemIndex::default();
        self.file_content.reset();


        // Refresh parent
        self.init_parent_files()?;
        list_state_select(
            &mut self.selected_item.parent,
            &mut self.parent_files,
            &mut select_prev[0],
            if root && target.is_some() {
                target.to_owned()
            } else {
                Some(parent_file)
            }
        );


        // Refresh current
        if root {
            let parent_filesaver = self.get_file_saver()
                .expect("Error 1 at update_with_prev_selected in app.rs.");

            if !parent_filesaver.is_dir {
                self.set_file_content()?;
                return Ok(())
            }

            self.init_current_files()?;
            if !select_prev[0] {
                self.select_normal(SearchFile::Current);
                return Ok(())
            }

            list_state_select(
                &mut self.selected_item.current,
                &mut self.current_files,
                &mut select_prev[1],
                current_file
            );

            return Ok(())
        }

        self.init_current_files()?;
        if select_prev[0] {
            list_state_select(
                &mut self.selected_item.current,
                &mut self.current_files,
                &mut select_prev[1],
                if target.is_some() {
                    target
                } else {
                    current_file
                }
            );
        } else {
            self.select_normal(SearchFile::Current);
        }


        // Refresh child or show file content
        self.child_files.clear();
        if self.current_files.is_empty() {
            return Ok(())
        }

        self.init_child_files()?; // Set file content is included by this func.
        if select_prev[1] {
            list_state_select(
                &mut self.selected_item.child,
                &mut self.child_files,
                &mut select_prev[1], // It's doesn't matter what thing is here.
                child_file
            );
        } else {
            self.select_normal(SearchFile::Child);
        }

        Ok(())
    }

    pub fn hide_or_show(&mut self, target: Option<String>) -> AppResult<()> {
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
impl<'a> App<'a> {
    pub fn set_file_content(&mut self) -> anyhow::Result<()> {
        use io::ErrorKind;

        let selected = self.get_file_saver();
        if let Some(selected_item) = selected {
            let selected_file = selected_item.to_owned();
            let file_path = self.current_path()
                .join(&selected_file.name);
            let mut content = Text::default();

            // To avoid the wrong display of file content caused by
            // image decoding delay.
            if !self.image_preview.useless {
                self.image_preview.useless = true;
            }

            match fs::File::open(file_path.to_owned()) {
                Err(e) => {
                    match e.kind() {
                        ErrorKind::NotFound => (),
                        ErrorKind::PermissionDenied => {
                            content = Text::raw("Permission Denied");
                        },
                        _ => return Err(e.into())
                    }
                },
                Ok(ref mut file) => {
                    if selected_file.is_file {
                        if let Err(_) = read_to_text(&mut content, file) {
                            // if e.kind() != io::ErrorKind::InvalidData {
                            //     return Err(e.into())
                            // }

                            // Try to display image file
                            if self.image_preview.with_image_feat() {
                                // The preview thread runs asynchronously: blank
                                // the pane so the previous file's preview
                                // cannot be taken for this one meanwhile.
                                self.file_content = FileContent::Text(Text::default());
                                self.image_preview.send_path(file_path)?;
                                self.image_preview.useless = false;

                                return Ok(())
                            }

                            content = Text::raw(file_type_label(&file_path));
                        }
                    } else {
                        content = Text::raw("Non Normal File");
                    }
                },
            };
            self.file_content = FileContent::Text(content);
        }

        Ok(())
    }

    /// Apply the preview the preview thread produced for `path`.
    ///
    /// The thread runs asynchronously, so the selection may have moved on (to
    /// another file, or to a directory) while it was busy. A late result must
    /// never replace the current view: it used to paint a file's type over a
    /// directory's listing, or over the next file's preview.
    pub fn apply_preview(&mut self, path: PathBuf, preview: Preview) {
        if self.selected_file_path().as_deref() != Some(path.as_path()) {
            return
        }

        match preview {
            Preview::Image(image) => {
                match self.image_preview.make_protocol(image) {
                    Err(err) => self.app_error.add_error(err),
                    Ok(_) => self.file_content = FileContent::Image,
                }
            },

            Preview::FileType(label) => {
                self.file_content = FileContent::Text(Text::raw(label));
            }
        }
    }

    /// The full path of the selected file.
    ///
    /// `None` for a directory (or an empty list): the third column shows the
    /// directory's contents then, never a file preview.
    fn selected_file_path(&self) -> Option<PathBuf> {
        let file = self.get_file_saver()?;

        if !file.is_file {
            return None
        }

        Some(self.current_path().join(&file.name))
    }

    fn read_files(&mut self, path: &Path) -> io::Result<Vec<FileSaver>> {
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

// Other Action
impl<'a> App<'a> {
    /// Jump to `dir` directory.
    pub fn goto_dir<P: AsRef<Path>>(&mut self,
                                    dir: P,
                                    hide_files: Option<bool>
    ) -> AppResult<()>
    {
        self.path = PathBuf::from(dir.as_ref());
        self.selected_item = ItemIndex::default();
        self.file_content.reset();
        self.child_files.clear();

        self.hide_files = if let Some(hide) = hide_files {
            hide
        } else {
            !path_is_hidden(&self.path)
        };

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
        let path = self.path.to_owned();

        self.marked_files
            .entry(path)
            .or_insert(MarkedFiles::default())
            .files
            .insert(file.into(), is_dir);
    }

    pub fn append_marked_files<I>(&mut self, iter: I)
    where I: Iterator<Item = FileSaver>
    {
        let path = self.path.to_owned();

        let temp_set = self.marked_files
            .entry(path)
            .or_insert(MarkedFiles::default());
        for file in iter {
            temp_set.files.insert(file.name, file.is_dir);
        }
    }

    pub fn marked_file_contains<S: Into<String>>(&self, file: S) -> bool {
        let path = self.path.to_owned();
        if let Some(marked_files) = self.marked_files.get(&path) {
            // In Linux, there could not be more than one files that have the same name.
            // (Include directories)
            return marked_files.files.contains_key(&file.into())
        }

        false
    }

    pub fn remove_marked_file<S: Into<String>>(&mut self, file: S) {
        let path = self.path.to_owned();
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

/// Check whether `path` is in hidden directories.
pub fn path_is_hidden<P: AsRef<Path>>(path: P) -> bool {
    let path = path.as_ref().to_string_lossy();
    path.contains("/.")
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

/// Select item in parent/current/child file.
/// Return whether passed the name of previous name.
#[inline]
fn list_state_select(
    item: &mut ListState,
    file_list: &mut Vec<FileSaver>,
    keep_prev: &mut bool,
    file_name: Option<String>
)
{
    if let Some(name) = file_name {
        match file_list.iter().position(|f| f.name == name) {
            Some(idx) => {
                item.select(Some(idx));
            },
            None => {
                *keep_prev = false;
                item.select(if file_list.is_empty() {
                    None
                } else {
                    Some(0)
                });
            },
        }

        return ()
    }

    if !file_list.is_empty() {
        item.select(Some(0));
    }
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
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir().join(format!(
            "hire-app-{}-{}-{}",
            std::process::id(),
            tag,
            nanos
        ));

        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The preview thread may answer after the selection moved on: a result
    /// for another file must never replace the current preview.
    #[test]
    fn test_stale_preview_is_dropped() {
        let dir = temp_dir("stale");
        fs::write(dir.join("a.txt"), "text").unwrap();
        fs::write(dir.join("b.txt"), "text").unwrap();

        let mut app = App::default();
        app.goto_dir(&dir, Some(false)).unwrap();
        app.file_content = FileContent::Text(Text::raw("before"));

        // `a.txt` is selected; this result belongs to `b.txt`.
        app.apply_preview(
            app.current_path().join("b.txt"),
            Preview::FileType(String::from("text/x-b"))
        );
        assert!(app.file_content == FileContent::Text(Text::raw("before")));

        // The result for the selected file is applied.
        app.apply_preview(
            app.current_path().join("a.txt"),
            Preview::FileType(String::from("text/x-a"))
        );
        assert!(app.file_content == FileContent::Text(Text::raw("text/x-a")));

        fs::remove_dir_all(&dir).unwrap();
    }

    /// A directory's listing must never be covered by a late file preview.
    #[test]
    fn test_preview_does_not_cover_a_directory_listing() {
        let dir = temp_dir("dir");
        fs::create_dir(dir.join("folder")).unwrap();
        fs::write(dir.join("folder").join("inner.txt"), "text").unwrap();
        fs::write(dir.join("a.txt"), "text").unwrap();

        let mut app = App::default();
        app.goto_dir(&dir, Some(false)).unwrap();

        // Directories sort first, so the directory is selected and the third
        // column shows its contents: there is no file preview to replace.
        assert!(!app.file_content.is_some());

        app.apply_preview(
            dir.join("a.txt"),
            Preview::FileType(String::from("text/plain"))
        );
        assert!(!app.file_content.is_some());

        fs::remove_dir_all(&dir).unwrap();
    }
}
