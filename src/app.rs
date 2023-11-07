// App

use std::path::PathBuf;
use std::{env, fs, io};

pub struct App {
    pub path: PathBuf,
    pub selected_item: (usize, usize),
    pub parent_files: Vec<String>,
    pub current_files: Vec<String>,
    pub child_files: Vec<String>,

    // TODO: Replace String with Cow<'a, str>
    pub computer_name: String,
    pub user_name: String
}

impl Default for App {
    fn default() -> Self {
        let current_dir = env::current_dir()
            .expect("Cannot get current directory!");
        let host_info = get_host_info();
        App {
            path: current_dir,
            selected_item: (0, 0),
            parent_files: Vec::new(),
            current_files: Vec::new(),
            child_files: Vec::new(),
            computer_name: host_info.0,
            user_name: host_info.1
        }
    }
}

impl App {
    /// Initialize parent, current and child files.
    pub fn init_all_files(&mut self) -> io::Result<()> {
        let temp_path = self.path.as_path();
        
        let mut parent_files: Vec<String> = fs::read_dir(
            temp_path.parent().expect("Cannot get parent dir of current directory!")
        )?
            .map(|ele| match ele {
                Ok(x) => x.file_name().into_string().unwrap(),
                Err(_) => panic!("Cannot get a file with error!")
            })
            .collect();
        // TODO: Select the parent dir.
        parent_files.sort();

        if parent_files.is_empty() {
            return Ok(())
        }

        // Current files
        let mut current_files: Vec<String> = fs::read_dir(
            temp_path
        )?
            .map(|ele| match ele {
                Ok(x) => x.file_name().into_string().unwrap(),
                Err(_) => panic!("Cannot get a file with error!")
            })
            .collect();
        current_files.sort();

        // Child Files
        let current_select = temp_path
            .join(current_files.get(0).unwrap());

        if current_select.is_dir() {
            let mut child_files: Vec<String> = fs::read_dir(
                temp_path.join(current_files.get(0).unwrap())
            )?
                .map(|ele| match ele {
                    Ok(x) => x.file_name().into_string().unwrap(),
                    Err(_) => panic!("Cannot get a file with error!")
                })
                .collect();
            child_files.sort();

            self.child_files = child_files;
        }

        // TODO: Replace with sort by method
        self.parent_files = parent_files;
        self.current_files = current_files;
        self.refresh_select_item();
        
        Ok(())
    }

    /// Refresh the selected item of parent dir & current file.
    pub fn refresh_select_item(&mut self) {
        let parent_dir = self.path.file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let idx = self.parent_files.iter().position(|e| *e == parent_dir).unwrap();
        self.selected_item.0 = idx;
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
