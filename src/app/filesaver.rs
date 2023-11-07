// FileSaver

/// The structure used to save file information.
#[derive(Eq, Ord, PartialEq, PartialOrd, Debug, Clone)]
pub struct FileSaver {
    pub name: String,
    pub is_dir: bool
}

impl FileSaver {
    pub fn new(name: String, is_dir: bool) -> FileSaver {
        // Temporarily regard the file as a normal file.
        FileSaver {
            name,
            is_dir
        }
    }
}

pub fn sort(files: &mut Vec<FileSaver>) {
    let mut directories: Vec<FileSaver> = Vec::new();
    let mut normal_files: Vec<FileSaver> = Vec::new();
    for file in files.iter() {
        // let mut  = expression;
        // match file.name.chars().nth(0).unwrap() {
        //     'A'..='Z' => {
        //     },
        //     _ => ()
        // }
        if file.is_dir {
            directories.push((*file).clone());
        } else {
            normal_files.push((*file).clone());
        }
    }
    directories.sort();
    normal_files.sort();
    directories.extend(normal_files);

    *files = directories;
}
