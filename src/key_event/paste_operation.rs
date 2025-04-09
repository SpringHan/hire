// Paste operation.

use std::fs;
use std::io::ErrorKind;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::SwitchCase;
use super::file_operations::delete_file;

use crate::{rt_error, App};
use crate::utils::{CmdContent, CursorPos, MarkedFiles};
use crate::error::{
    NotFoundType,
    ErrorType,
    AppResult,
    AppError,
};

macro_rules! append_error {
    ($errors:expr, $x:expr) => {
        let (_, _errs) = $x;
        if !_errs.is_empty() {
            $errors.append_errors(_errs.iter());
        }
    };
}

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

/// Paste files and return failed files & errors.
pub fn paste_files<'a, I, P>(file_iter: I,
                             target_path: P,
                             overwrite: bool
) -> (HashMap<PathBuf, Vec<String>>, AppError)
where
    I: Iterator<Item = (&'a PathBuf, &'a MarkedFiles)>,
    P: AsRef<Path>
{
    use copy_dir::copy_dir;

    let mut errors = AppError::new();
    let mut failed_files: HashMap<PathBuf, Vec<String>> = HashMap::new();

    macro_rules! file_action {
        ($func:expr, $path: expr, $file:expr, $from:expr $(, $to:expr )*) => {
            match $func($from, $( $to )*) {
                Err(err) => {
                    failed_files.entry($path.to_owned())
                        .or_insert(Vec::new())
                        .push($file.0.to_owned());

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
            let mut final_path = target_path.as_ref().join(file.0);
            let target_file = fs::metadata(&final_path);
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
                        final_path = target_path.as_ref().join(
                            format!("{}.new", file.0)
                        );
                        // exists_files
                        //     .entry(path.to_owned())
                        //     .or_insert(Vec::new())
                        //     .push(file.0.to_owned());
                    } else {
                        target_exists = true;
                        target_is_dir = metadata.is_dir();
                    }
                }
            }

            // NOTE: If the exists_file is a directory and the original one is not, cancel this action
            // and vice versa.
            if target_exists {
                if target_is_dir {
                    file_action!(
                        fs::remove_dir_all,
                        path,
                        file,
                        target_path.as_ref().join(&file.0)
                    );
                } else {
                    file_action!(
                        fs::remove_file,
                        path,
                        file,
                        target_path.as_ref().join(&file.0)
                    );
                }
            }

            if *file.1 {         // The original file is a dir.
                file_action!(
                    copy_dir,
                    path,
                    file,
                    path.join(&file.0),
                    final_path
                );
            } else {
                file_action!(
                    fs::copy,
                    path,
                    file,
                    path.join(&file.0),
                    final_path
                );
            }
        }
    }

    (failed_files, errors)
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
    let mut errors = AppError::new();
    let current_dir = app.current_path();
    let files = app.marked_files.to_owned();

    match key {
        'p' => {
            let (failed_files, mut _errs) = paste_files(
                files.iter(),
                current_dir,
                false
            );

            if !_errs.is_empty() {
                errors.append_errors(_errs.iter());
            }
            
            if let Err(err) = remove_origin_files(
                app,
                files.into_iter(),
                failed_files
            )
            {
                errors.append_errors(err.iter());
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
            append_error!(errors, paste_files(
                files.iter(),
                current_dir,
                false
            ));
        },

        'o' => {
            append_error!(errors, paste_files(
                files.iter(),
                current_dir,
                true
            ));
        },

        'O' => {
            let (failed_files, _errs) = paste_files(
                files.iter(),
                current_dir,
                true
            );

            if !_errs.is_empty() {
                errors.append_errors(_errs.iter());
            }

            if let Err(err) = remove_origin_files(
                app,
                files.into_iter(),
                failed_files
            )
            {
                errors.append_errors(err.iter());
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

    if !errors.is_empty() {
        return Err(errors)
    }

    Ok(true)
}

fn remove_origin_files<I>(
    app: &mut App,
    files: I,
    failed_files: HashMap<PathBuf, Vec<String>>
) -> AppResult<()>
where I: Iterator<Item = (PathBuf, MarkedFiles)>
{
    for (path, files) in files {
        // Avoid removing files that failed to be moved to target path.
        let marked_ref = app.marked_files.get_mut(&path).unwrap();
        let mut temp_files: HashMap<String, bool>;

        if let Some(failed) = failed_files.get(&path) {
            temp_files = HashMap::new();

            for (name, is_dir) in files.files.into_iter() {
                if !failed.contains(&name) {
                    marked_ref.files.remove(&name);
                    temp_files.entry(name).or_insert(is_dir);
                }
            }
        } else {
            temp_files = files.files;
            marked_ref.files.clear();
        };

        delete_file(
            app,
            path,
            temp_files.into_iter(),
            false,
            false       // Not necesary
        )?;
    }

    Ok(())
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
    app.mark_expand = false;
    app.marked_files.clear();
    app.goto_dir(app.current_path(), None)?;

    Ok(())
}
