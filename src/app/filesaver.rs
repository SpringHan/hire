// FileSaver

/// The structure used to save file information.
#[derive(Eq, Ord, PartialEq, PartialOrd, Debug)]
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

// pub fn sort(files: &mut Vec<FileSaver>) {
//     let mut temp_files: Vec<String> = files.iter().map(|s| s.name.clone()).collect();
//     temp_files.sort();
//     let mut i = 0;
// }
