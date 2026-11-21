// Paste operation.

use std::fs;
use std::io::ErrorKind;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ratatui::text::{Line, Text};

use super::{MenuPresentation, SwitchCase};
use super::file_operations::delete_file;
use crate::{rt_error, App};
use crate::ui::key_hints;
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
        MenuPresentation::Dialog,
        "Paste",
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

    // A copy or a symlink keeps the marked files (see `restore_status`), so
    // the same selection is ready for the next paste; a move consumes its
    // source files, and `x` (clear the selection) drops the marks.
    let mut keep_marked_files = false;

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

            keep_marked_files = true;
        },

        'c' => {
            append_error!(errors, paste_files(
                files.iter(),
                current_dir,
                false
            ));

            keep_marked_files = true;
        },

        'o' => {
            append_error!(errors, paste_files(
                files.iter(),
                current_dir,
                true
            ));

            keep_marked_files = true;
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

    restore_status(app, keep_marked_files)?;

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
    let keys = app.term_colors.marked_style;
    let mut text = Text::default();

    text.push_line(key_hints(
        "[p] move to here  [s] make symbolic link  [c] copy to here",
        keys
    ));
    text.push_line(key_hints(
        "[o] copy to here forcely  [O] move to here forcely  \
         [x] clear selected files",
        keys
    ));
    text.push_line(Line::raw(""));
    text.push_line(Line::raw("Selected files:"));

    for (path, files) in app.marked_files.iter() {
        for (file, is_dir) in files.files.iter() {
            let mut line = format!(
                "{}/{}",
                if path.to_string_lossy() == "/" {
                    String::new()
                } else {
                    path.to_string_lossy().to_string()
                },
                file
            );

            if *is_dir {
                line.push('/');
            }

            text.push_line(Line::raw(line));
        }
    }

    CmdContent::Text(text)
}

/// Refresh the view after the paste menu and clear the marks unless they are
/// kept.
///
/// `keep_marked_files` is set by the copy and symlink entries: their sources
/// stay where they are, so the marks can be reused for the next paste. A move
/// consumes its sources and the clear entry exists to drop them, so both still
/// clear the marks.
fn restore_status(app: &mut App, keep_marked_files: bool) -> AppResult<()> {
    app.mark_expand = false;

    if !keep_marked_files {
        app.marked_files.clear();
    }

    app.goto_dir(app.current_path(), Some(app.hide_files))?;

    Ok(())
}

#[cfg(test)]
mod test {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{key_event::switch::switch_match, utils::MarkedFiles};

    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir().join(format!(
            "hire-paste-{}-{}-{}",
            std::process::id(),
            tag,
            nanos
        ));

        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Mark `name` in `from` and answer the paste menu with `key` in `target`.
    fn paste(app: &mut App, from: &Path, name: &str, target: &Path, key: char) {
        app.goto_dir(target, Some(false)).unwrap();

        let mut marked = MarkedFiles::default();
        marked.files.insert(name.to_owned(), false);
        app.marked_files.insert(from.to_path_buf(), marked);

        paste_operation(app).unwrap();

        let case = app.switch_case.clone().expect("the paste menu is open");
        switch_match(app, case, key).unwrap();
    }

    fn marks_contain(app: &App, from: &Path, name: &str) -> bool {
        app.marked_files
            .get(from)
            .is_some_and(|marked| marked.files.contains_key(name))
    }

    /// `c` copies and keeps the selection, so the same files can be pasted
    /// again; the originals stay where they are.
    #[test]
    fn test_copy_keeps_the_marked_files() {
        let source = temp_dir("copy-source");
        let target = temp_dir("copy-target");
        fs::write(source.join("a.txt"), "new").unwrap();

        let mut app = App::default();
        paste(&mut app, &source, "a.txt", &target, 'c');

        assert_eq!(fs::read_to_string(target.join("a.txt")).unwrap(), "new");
        assert!(source.join("a.txt").exists());
        assert_eq!(app.marked_files.len(), 1);
        assert!(marks_contain(&app, &source, "a.txt"));

        fs::remove_dir_all(&source).unwrap();
        fs::remove_dir_all(&target).unwrap();
    }

    /// `o` overwrites the target and keeps the selection as well.
    #[test]
    fn test_copy_with_overwrite_keeps_the_marked_files() {
        let source = temp_dir("overwrite-source");
        let target = temp_dir("overwrite-target");
        fs::write(source.join("a.txt"), "new").unwrap();
        fs::write(target.join("a.txt"), "old").unwrap();

        let mut app = App::default();
        paste(&mut app, &source, "a.txt", &target, 'o');

        assert_eq!(fs::read_to_string(target.join("a.txt")).unwrap(), "new");
        assert_eq!(app.marked_files.len(), 1);
        assert!(marks_contain(&app, &source, "a.txt"));

        fs::remove_dir_all(&source).unwrap();
        fs::remove_dir_all(&target).unwrap();
    }

    /// `s` creates the symbolic links and keeps the selection as well, so the
    /// same files can be linked somewhere else next.
    #[test]
    fn test_symlink_keeps_the_marked_files() {
        let source = temp_dir("symlink-source");
        let target = temp_dir("symlink-target");
        fs::write(source.join("a.txt"), "new").unwrap();

        let mut app = App::default();
        paste(&mut app, &source, "a.txt", &target, 's');

        let link = target.join("a.txt");
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_link(&link).unwrap(), source.join("a.txt"));
        assert_eq!(app.marked_files.len(), 1);
        assert!(marks_contain(&app, &source, "a.txt"));

        fs::remove_dir_all(&source).unwrap();
        fs::remove_dir_all(&target).unwrap();
    }

    /// A move keeps the old behaviour: the source is gone, and so are its
    /// marks.
    #[test]
    fn test_move_clears_the_marked_files() {
        let source = temp_dir("move-source");
        let target = temp_dir("move-target");
        fs::write(source.join("a.txt"), "new").unwrap();

        let mut app = App::default();
        paste(&mut app, &source, "a.txt", &target, 'p');

        assert_eq!(fs::read_to_string(target.join("a.txt")).unwrap(), "new");
        assert!(!source.join("a.txt").exists());
        assert!(app.marked_files.is_empty());

        fs::remove_dir_all(&source).unwrap();
        fs::remove_dir_all(&target).unwrap();
    }

    /// `O` moves with overwrite: the target is replaced and the marks go away.
    #[test]
    fn test_move_with_overwrite_clears_the_marked_files() {
        let source = temp_dir("force-move-source");
        let target = temp_dir("force-move-target");
        fs::write(source.join("a.txt"), "new").unwrap();
        fs::write(target.join("a.txt"), "old").unwrap();

        let mut app = App::default();
        paste(&mut app, &source, "a.txt", &target, 'O');

        assert_eq!(fs::read_to_string(target.join("a.txt")).unwrap(), "new");
        assert!(!source.join("a.txt").exists());
        assert!(app.marked_files.is_empty());

        fs::remove_dir_all(&source).unwrap();
        fs::remove_dir_all(&target).unwrap();
    }

    /// `x` is the "clear selected files" entry, so it still drops the marks
    /// and copies nothing.
    #[test]
    fn test_clear_selection_clears_the_marked_files() {
        let source = temp_dir("clear-source");
        let target = temp_dir("clear-target");
        fs::write(source.join("a.txt"), "new").unwrap();

        let mut app = App::default();
        paste(&mut app, &source, "a.txt", &target, 'x');

        assert!(!target.join("a.txt").exists());
        assert!(app.marked_files.is_empty());

        fs::remove_dir_all(&source).unwrap();
        fs::remove_dir_all(&target).unwrap();
    }
}
