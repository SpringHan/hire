// Paste operation.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

use super::SwitchCase;
use super::file_operations::delete_file;

use crate::{rt_error, App};
use crate::app::{CmdContent, CursorPos, MarkedFiles};
use crate::error::{
    AppResult,
    AppError,
    ErrorType,
    NotFoundType
};

pub fn paste_operation(app: &mut App) -> AppResult<()> {
    if app.marked_files.is_empty() {
        return Err(ErrorType::NoSelected.pack())
    }

    SwitchCase::new(
        app,
        paste_switch,
        true,
        generate_msg(app),
        super::SwitchCaseData::None
    );

    Ok(())
}

pub fn paste_files<'a, I, P>(file_iter: I,
                             target_path: P,
                             overwrite: bool
) -> AppResult<HashMap<PathBuf, Vec<String>>>
where
    I: Iterator<Item = (&'a PathBuf, &'a MarkedFiles)>,
    P: AsRef<Path>
{
    use copy_dir::copy_dir;

    let mut errors = AppError::new();
    let mut exists_files: HashMap<PathBuf, Vec<String>> = HashMap::new();

    macro_rules! file_action {
        ($func:expr, $file:expr, $from:expr $(, $to:expr )*) => {
            match $func($from, $( $to )*) {
                Err(err) => {
                    errors.add_error(err);
                    continue;
                },
                Ok(_) => ()
            }
        }
    }

    for (path, files) in file_iter {
        let mut target_exists = false;
        let mut target_is_dir = false;

        for file in files.files.iter() {
            let target_file = fs::metadata(
                target_path.as_ref().join(file.0)
            );
            // Check whether the target file exists.
            match target_file {
                Err(err) => {
                    match err.kind() {
                        ErrorKind::NotFound => (), // Nice find.
                        _ => {
                            errors.add_error(err);
                        }
                    }
                },
                Ok(metadata) => {
                    if !overwrite {
                        exists_files
                            .entry(path.to_owned())
                            .or_insert(Vec::new())
                            .push(file.0.to_owned());
                        continue;
                    }
                    target_exists = true;
                    target_is_dir = metadata.is_dir();
                }
            }

            // NOTE: If the exists_file is a directory and the original one is not, cancel this action
            // and vice versa.
            if target_exists {
                if target_is_dir {
                    file_action!(
                        fs::remove_dir_all,
                        file,
                        target_path.as_ref().join(&file.0)
                    );
                } else {
                    file_action!(
                        fs::remove_file,
                        file,
                        target_path.as_ref().join(&file.0)
                    );
                }
            }

            if *file.1 {         // The original file is a dir.
                file_action!(
                    copy_dir,
                    file,
                    path.join(&file.0),
                    target_path.as_ref().join(&file.0)
                );
            } else {
                file_action!(
                    fs::copy,
                    file,
                    path.join(&file.0),
                    target_path.as_ref().join(&file.0)
                );
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors)
    }

    Ok(exists_files)
}

pub fn make_single_symlink(app: &mut App) -> AppResult<()> {
    if app.marked_files.is_empty() {
        return Err(ErrorType::NoSelected.pack())
    }

    if app.marked_files.len() > 1 {
        rt_error!("The number of marked files is more than one")
    }

    for (path, files) in app.marked_files.iter() {
        for (file, _) in files.files.iter() {
            let original_file = path.join(file);
            app.selected_block.set_command_line(
                format!(
                    ":create_symlink {} {}",
                    original_file.to_string_lossy(),
                    app.current_path().join(file).to_string_lossy()
                ),
                CursorPos::End
            );
            return Ok(())
        }
    }

    Ok(())
}

fn paste_switch(
    app: &mut App,
    key: char,
    _: super::SwitchCaseData
) -> AppResult<bool>
{
    let current_dir = app.current_path();
    let files = app.marked_files.to_owned();

    match key {
        'p' => {
            let exists_files = paste_files(
                files.iter(),
                current_dir,
                false
            )?;

            for (path, files) in files.into_iter() {
                // Avoid removing files that failed to be moved to target path.
                let path_in_exists = exists_files.get(&path);
                let files: HashMap<String, bool> = if
                    let Some(exists) = path_in_exists
                {
                    files.files
                        .into_iter()
                        .filter(|file|
                                !exists.contains(&file.0))
                        .collect()
                } else {
                    files.files
                };

                delete_file(
                    app,
                    path,
                    files.into_iter(),
                    false,
                    false       // Not necesary
                )?;
            }

            let mut files_for_error: Vec<String> = Vec::new();
            for (_, files) in exists_files.into_iter() {
                files_for_error.extend(files);
            }

            if !files_for_error.is_empty() {
                restore_status(app)?;
                return Err(ErrorType::FileExists(files_for_error).pack())
            }
        },
        's' => {
            let mut final_files: Vec<(PathBuf, PathBuf)> = Vec::new();
            for (path, files) in files.into_iter() {
                for (file, _) in files.files.into_iter() {
                    final_files.push((path.join(&file), current_dir.join(file)));
                }
            }

            crate::command::create_symlink(app, final_files.into_iter())?;
        },
        'c' => {
            paste_files(
                files.iter(),
                current_dir,
                false
            )?;
        },
        'o' => {
            paste_files(
                files.iter(),
                current_dir,
                true
            )?;
        },
        'O' => {
            paste_files(
                files.iter(),
                current_dir,
                true
            )?;

            for (path, files) in files.into_iter() {
                delete_file(
                    app,
                    path,
                    files.files.into_iter(),
                    false,
                    false       // Not necesary
                )?;
            }
        },
        'x' => (),
        _ => {
            return Err(
                ErrorType::NotFound(
                    NotFoundType::Item(format!("key {}", key))
                ).pack()
            )
        }
    }

    restore_status(app)?;

    Ok(true)
}

fn generate_msg(app: &App) -> CmdContent {
    let mut msg = String::from("[p] move to here  [s] make symbolic link  [c] copy to here
[o] copy to here forcely  [O] move to here forcely  [x] clear selected files
\nSelected files:\n");

    for (path, files) in app.marked_files.iter() {
        for (file, is_dir) in files.files.iter() {
            msg.push_str(&format!(
                "{}/{}",
                if path.to_string_lossy() == "/" {
                    String::new()
                } else {
                    path.to_string_lossy().to_string()
                },
                file
            ));

            if *is_dir {
                msg.push('/');
            }

            msg.push('\n');
        }
    }

    CmdContent::String(msg)
}

fn restore_status(app: &mut App) -> AppResult<()> {
    app.marked_files.clear();
    app.goto_dir(app.current_path(), None)?;

    Ok(())
}
