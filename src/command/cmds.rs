// Command functions

use std::fs;
use std::path::{PathBuf, Path};

use crate::App;
use crate::rt_error;
use crate::app::{sort, FileSaver};
use crate::error::{AppResult, AppError, ErrorType};

pub fn rename_file(
    path: PathBuf,
    app: &mut App,
    new_name: String
) -> AppResult<()>
{
    let hide_files = app.hide_files;
    let file = app.get_file_saver_mut();
    if let None = file {
        return Err(ErrorType::NoSelected.pack());
    }

    let file = file.unwrap();

    if file.cannot_read || file.read_only() {
        rt_error!("Permission denied")
    }

    if file_exists(path.join(new_name.to_owned()))? {
        rt_error!("{new_name} already exists")
    }

    let origin_file = path.join(&file.name);
    let new_file = path.join(&new_name);
    fs::rename(origin_file, &new_file)?;
    file.name = new_name.to_owned();

    if new_name.starts_with(".") && hide_files {
        app.hide_or_show(Some(new_name))?;
        return Ok(())
    }

    // Refresh modified time
    let metadata = fs::metadata(new_file)?;
    file.set_modified(metadata.modified().unwrap());

    // Refresh the display of whole directory
    let (directory, index) = app.get_directory_mut();
    let mut new_files = directory.to_owned();
    sort(&mut new_files);
    let new_index = new_files
        .iter()
        .position(|x| x.name == new_name)
        .unwrap();
    *directory = new_files;
    index.select(Some(new_index));

    Ok(())
}

pub fn create_file<'a, I>(
    app: &mut App,
    files: I,
    is_dir: bool
) -> AppResult<()>
where I: Iterator<Item = &'a str>
{
    let path = app.current_path();
    let mut errors = AppError::new();

    let mut new_files: Vec<FileSaver> = Vec::new();
    let mut to_show_hidden_files = false;

    for file in files {
        let mut file = file.trim_start().to_owned();
        if file_exists(path.join(&file))? {
            file.push_str(".new");
        }

        if is_dir {
            match fs::create_dir(path.join(&file)) {
                Err(err) => {
                    errors.add_error(err);
                },
                Ok(_) => new_files.push(FileSaver::new(
                    file.to_owned(),
                    path.join(&file),
                    None
                ))
            }
        } else {
            let file_create = fs::File::create(
                path.join(&file)
            );
            match file_create {
                Ok(file_create) => {
                    new_files.push(FileSaver::new(
                        file.to_owned(),
                        path.join(&file),
                        Some(file_create.metadata())
                    ));
                },
                Err(err) => {
                    errors.add_error(err);
                }
            }
        }

        if !to_show_hidden_files && file.starts_with(".") && app.hide_files {
            to_show_hidden_files = true;
        }
    }

    // Update render
    if !to_show_hidden_files {
        let mut prev_file_name = String::from("");
        let current_file = app.get_file_saver();
        if let Some(current_file) = current_file {
            prev_file_name = current_file.name.to_owned();
        }

        let (dir, idx) = app.get_directory_mut();
        dir.extend(new_files.into_iter());
        sort(dir);
        if prev_file_name.is_empty() {
            idx.select(Some(0));
            if app.path.to_string_lossy() == "/" {
                // NOTE: The first item in root directory must be a dir.
                app.init_current_files()?;
                app.selected_item.current_select(Some(0));
            } else {
                app.init_child_files()?;
                app.refresh_select_item();
            }
        } else {
            idx.select(Some(
                dir.iter()
                    .position(|file|
                              file.name == prev_file_name)
                    .unwrap()
            ));
            if app.path.to_string_lossy() == "/" {
                app.init_current_files()?;
                app.selected_item.current_select(Some(0));
            } else {
                app.init_child_files()?;
                app.refresh_select_item();
            }
        }
    } else {
        app.hide_or_show(None)?;
    }

    if !errors.is_empty() {
        return Err(errors)
    }

    Ok(())
}

pub fn create_symlink<I, P>(app: &mut App, files: I) -> AppResult<()>
where
    I: Iterator<Item = (P, P)>,
    P: AsRef<Path>
{
    use std::os::unix::fs::symlink;

    let mut errors = AppError::new();
    let mut to_show_hidden_files = false;

    for (file, target) in files {
        match symlink(&file, &target) {
            Err(err) => {
                errors.add_error(err);
            },
            _ => {
                if !to_show_hidden_files
                    && target.as_ref().file_name().unwrap().to_string_lossy().starts_with(".")
                    && app.hide_files
                {
                    to_show_hidden_files = true;
                }
            }
        }
    }
    if to_show_hidden_files {
        app.hide_or_show(None)?;
    } else {
        app.partly_update_block()?;
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(())
}

pub fn file_exists(file: PathBuf) -> std::io::Result<bool> {
    match fs::metadata(file) {
        Ok(_) => Ok(true),
        Err(err) => {
            if err.kind() == std::io::ErrorKind::NotFound {
                return Ok(false)
            }

            Err(err)
        }
    }
}
