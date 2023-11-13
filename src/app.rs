// App

pub mod filesaver;
use filesaver::{FileSaver, sort};

use std::path::{PathBuf, Path};
use std::{env, fs, io};
use std::borrow::Cow;

/// The type of block that can be selected.
/// The boolean from Browser indicates whether current dir is the root directory.
pub enum Block {
    Browser(bool),
    CommandLine(String)
}

pub struct ItemIndex {
    pub parent: Option<usize>,
    pub current: Option<usize>,
    pub child: Option<usize>
}

impl Default for ItemIndex {
    fn default() -> ItemIndex {
        ItemIndex {
            parent: None,
            current: None,
            child: None
        }
    }
}

pub struct App {
    pub path: PathBuf,
    pub selected_item: ItemIndex,
    pub parent_files: Vec<FileSaver>,
    pub current_files: Vec<FileSaver>,
    pub child_files: Vec<FileSaver>,

    // NOTE: When file_content is not None, child_files must be empty.
    pub file_content: Option<String>,

    pub selected_block: Block,

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
            selected_block: Block::Browser(false),
            computer_name: Cow::from(host_info.0),
            user_name: Cow::from(host_info.1)
        }
    }
}

impl App {
    /// Initialize parent, current and child files.
    pub fn init_all_files(&mut self) -> io::Result<()> {
        // Parent files
        self.init_parent_files()?;

        if self.parent_files.is_empty() {
            return Ok(())
        }

        // Current files
        self.init_current_files(None)?;
        if self.current_files.is_empty() {
            return Ok(())
        }

        // Child Files
        let current_selected_file = self.current_files.get(0).unwrap().clone();
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
        if let None = self.selected_item.parent {
            let parent_dir = self.path.file_name()
                .unwrap()
                .to_string_lossy();
            let idx = self.parent_files
                .iter()
                .position(|e| e.name == parent_dir).unwrap();
            self.selected_item.parent = Some(idx);
        }
        
        // Current
        if let None = self.selected_item.current {
            self.selected_item.current = Some(0);
        }
        
        if child_keep {
            return ()
        }

        // Child
        if !self.child_files.is_empty() {
            self.selected_item.child = Some(0);
        } else if self.selected_item.child.is_some() {
            self.selected_item.child = None;
        }
    }

    pub fn init_parent_files(&mut self) -> io::Result<()> {
        let temp_path = if let Some(path) = self.path.parent() {
            path.to_path_buf()
        } else {
            // Cannot get parent directory info at root dir.
            return Ok(())
        };

        let mut parent_files: Vec<FileSaver> = self.read_files(temp_path.as_path())?;
        sort(&mut parent_files);
        self.parent_files = parent_files;

        Ok(())
    }

    /// When the result is false, then stop init child files in the main function.
    /// The PATH is used when the user is in root directory.
    pub fn init_current_files(&mut self, path: Option<PathBuf>) -> io::Result<()> {
        // TODO: Rewrite the logic for changing CANNOT_READ. Make it happen in this function.
        let temp_path = if let Some(_path) = path {
            self.path.join(_path)
        } else {
            self.path.clone()
        };

        let mut current_files: Vec<FileSaver> = self.read_files(temp_path.as_path())?;
        if current_files.is_empty() {
            return Ok(())
        }

        sort(&mut current_files);

        self.current_files = current_files;
        Ok(())
    }

    /// To intialize child files, CURRENT_SELECT should be Some(FileSaver)
    /// To update child files, the value should be None.
    pub fn init_child_files(&mut self,
                            current_select: Option<&FileSaver>
    ) -> io::Result<()>
    {
        let temp_path = self.path.clone();
        let current_select = if let Some(file) = current_select {
            file
        } else {
            self.current_files.get(self.selected_item.current.unwrap()).unwrap()
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

    pub fn set_file_content(&mut self) -> io::Result<()> {
        use io::Read;

        let file_path = self.path.join(
            PathBuf::from(
                if let Block::Browser(true) = self.selected_block {
                    &self.parent_files.get(
                        self.selected_item.parent.unwrap()
                    )
                        .unwrap()
                        .name
                } else {
                    &self.current_files.get(
                        self.selected_item.current.unwrap()
                    )
                        .unwrap()
                        .name
                }
            )
        );
        let mut file = fs::File::open(file_path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        self.file_content = Some(content);

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
                    let temp_file = if let
                        Block::Browser(true) = self.selected_block
                    {
                        self.parent_files
                            .get_mut(self.selected_item.parent.unwrap())
                            .unwrap()
                    } else {
                        self.current_files
                            .get_mut(self.selected_item.current.unwrap())
                            .unwrap()
                    };
                    temp_file.cannot_read = true;
                    Ok(Vec::new())
                } else {
                    Err(err)
                }
            },
        }
    }
}

#[inline]
fn filesave_closure(ele: Result<fs::DirEntry, io::Error>) -> FileSaver {
    match ele {
        Ok(x) => FileSaver::new(x),
        Err(_) => panic!("Cannot get a file with error!")
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
