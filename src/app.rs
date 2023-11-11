// App

pub mod filesaver;
use filesaver::{FileSaver, sort};

use std::path::PathBuf;
use std::{env, fs, io};
use std::borrow::Cow;

/// The type of block that can be selected.
pub enum Block {
    Browser,
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
            selected_block: Block::Browser,
            computer_name: Cow::from(host_info.0),
            user_name: Cow::from(host_info.1)
        }
    }
}

impl App {
    /// Initialize parent, current and child files.
    pub fn init_all_files(&mut self) -> io::Result<()> {
        let temp_path = self.path.as_path();
        
        let mut parent_files: Vec<FileSaver> = fs::read_dir({
            let temp = temp_path.parent();
            if let Some(path) = temp {
                path
            } else {
                // Cannot get parent directory info at root dir.
                return Ok(())
            }
        })?
            .map(filesave_closure)
            .collect();
        sort(&mut parent_files);

        if parent_files.is_empty() {
            return Ok(())
        }

        // Current files
        let mut current_files: Vec<FileSaver> = fs::read_dir(
            temp_path
        )?
            .map(filesave_closure)
            .collect();
        sort(&mut current_files);

        if current_files.is_empty() {
            self.parent_files = parent_files;
            return Ok(())
        }

        // Child Files
        self.init_child_files(
            Some(current_files.get(0).unwrap())
        )?;

        self.parent_files = parent_files;
        self.current_files = current_files;
        self.refresh_select_item();
        
        Ok(())
    }

    /// Refresh the selected item of parent dir & current file.
    pub fn refresh_select_item(&mut self) {
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

        // Child
        if !self.child_files.is_empty() {
            self.selected_item.child = Some(0);
        } else if self.selected_item.child.is_some() {
            self.selected_item.child = None;
        }
    }

    /// To intialize child files, CURRENT_SELECT should be Some(FileSaver)
    /// To update child files, the value should be None.
    pub fn init_child_files(&mut self,
                            current_select: Option<&FileSaver>
    ) -> io::Result<()> {
        let temp_path = self.path.as_path();
        let current_select = if let Some(file) = current_select {
            file
        } else {
            self.current_files.get(self.selected_item.current.unwrap()).unwrap()
        };

        if current_select.is_dir {
            let mut child_files: Vec<FileSaver> = fs::read_dir(
                temp_path.join(&current_select.name)
            )?
                .map(filesave_closure)
                .collect();
            sort(&mut child_files);

            self.child_files = child_files;
        } else if !self.child_files.is_empty() {
            self.child_files.clear();
            // TODO: Set selected file content.
        }

        Ok(())
    }

    fn set_file_content(&mut self) {
    }

}

#[inline]
fn filesave_closure(ele: Result<std::fs::DirEntry, std::io::Error>) -> FileSaver {
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
