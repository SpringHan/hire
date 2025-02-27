// Goto Operation.

use std::path::PathBuf;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};

use toml_edit::{value, Document};

use super::Goto;
use super::{SwitchCase, SwitchCaseData};

use crate::app::App;
use crate::error::{AppResult, ErrorType, NotFoundType};

pub fn goto_operation(app: &mut App) {
    SwitchCase::new(
        app,
        goto_switch,
        generate_msg(app, '\0'),
        SwitchCaseData::Char('\0')
    );
}

fn goto_switch(app: &mut App, key: char, data: SwitchCaseData) -> AppResult<bool> {
    let modify_type = if let SwitchCaseData::Char(_data) = data {
        _data
    } else {
        panic!("[1] Unexpected error at goto_switch function in goto_operation.rs.")
    };

    match key {
        'g' => super::cursor_movement::move_cursor(
            app,
            Goto::Index(0),
            app.path.to_string_lossy() == "/"
        )?,
        '+' => {
            SwitchCase::new(
                app,
                goto_switch,
                generate_msg(app, '+'),
                SwitchCaseData::Char('+')
            );

            return Ok(false)
        },
        '-' => {
            SwitchCase::new(
                app,
                goto_switch,
                generate_msg(app, '-'),
                SwitchCaseData::Char('-')
            );

            return Ok(false)
        },
        k => {
            match modify_type {
                '+' => {
                    add_target_dir(
                        app,
                        key,
                        if app.path.to_string_lossy() == "/" {
                            PathBuf::from("/")
                        } else {
                            app.current_path()
                        }
                    )?;
                },
                '-' => remove_target_dir(app, key)?,
                '\0' => {
                    let target_path = app.target_dir.get(&k).cloned();
                    if let Some(path) = target_path {
                        app.goto_dir(path, None)?;
                    }
                },
                _ => panic!("[2] Unexpected error at goto_switch in goto_operation.rs.")
            }
        }
    }

    Ok(true)
}

fn generate_msg(app: &App, status: char) -> String {
    let mut msg = String::from("[g] goto top  [+] add directory  [-] remove directory\n\n");
    match status {
        '+' => msg.insert_str(0, "Adding Directory!\n"),
        '-' => msg.insert_str(0, "Deleting Directory!\n"),
        _ => ()
    }

    for (key, path) in app.target_dir.iter() {
        msg.push_str(&format!("[{}] {}\n", key, path));
    }

    msg
}

fn add_target_dir(app: &mut App, key: char, path: PathBuf) -> io::Result<()> {
    let path = path.to_str().unwrap().to_owned();
    let mut file_open = OpenOptions::new();

    let mut read_file = file_open.read(true).open(&app.config_path)?;
    let mut config = String::new();
    read_file.read_to_string(&mut config)?;

    let mut toml_config: Document = if config.trim().is_empty() {
        Document::new()
    } else {
        config
            .parse()
            .expect("Failed to read config from toml file!")
    };
    toml_config["target_dir"][String::from(key)] = value(&path);

    let mut write_file = file_open
        .write(true)
        .truncate(true)
        .open(&app.config_path)?;
    write_file.write_all(toml_config.to_string().as_bytes())?;

    app.target_dir.entry(key).or_insert(path);

    Ok(())
}

fn remove_target_dir(app: &mut App, key: char) -> AppResult<()> {
    let mut file_open = OpenOptions::new();

    let mut read_file = file_open.read(true).open(&app.config_path)?;
    let mut config = String::new();
    read_file.read_to_string(&mut config)?;

    if config.trim().is_empty() {
        return Err(ErrorType::NotFound(NotFoundType::None).pack())
    }

    let mut toml_config: Document = config
        .parse()
        .expect("Failed to read config from toml file!");
    if let Some(target_dir) = toml_config.get_mut("target_dir") {
        target_dir[&String::from(key)] = toml_edit::Item::None;
    } else {
        return Err(ErrorType::NotFound(NotFoundType::None).pack())
    }

    let mut write_file = file_open
        .write(true)
        .truncate(true)
        .open(&app.config_path)?;
    write_file.write_all(toml_config.to_string().as_bytes())?;

    app.target_dir.remove(&key);

    Ok(())
}

/// Pass the config file path & concrete config into App.
pub fn init_config(app: &mut App) -> io::Result<()> {
    let file = create_config_file()?;

    let mut read_file = OpenOptions::new().read(true).open(&file)?;
    let mut config = String::new();
    read_file.read_to_string(&mut config)?;

    app.config_path = file;

    if config.trim().is_empty() {
        return Ok(())
    }

    let toml_config: Document = config
        .parse()
        .expect("Failed to parse config content into toml!");

    if let Some(inline_table) = toml_config.get("target_dir") {
        for e in inline_table
            .as_inline_table()
            .unwrap()
            .into_iter()
        {
            app.target_dir
                .entry(e.0.parse().unwrap())
                .or_insert(
                    e.1
                        .as_str()
                        .unwrap()
                        .trim()
                        .to_owned()
                );
        }
    }

    Ok(())
}

/// Create config file if it doesn't exist.
///
/// Then return the file path.
fn create_config_file() -> io::Result<String> {
    let user = std::env::var("USER").expect("Failed to get user name!");
    let config_dir = format!(
        "{}/.config/springhan/hire/",
        if user == String::from("root") {
            String::from("/root")
        } else {
            format!("/home/{}", user)
        }
    );
    let file_path = format!("{}config.toml", config_dir);

    if let Err(err) = File::open(&config_dir) {
        if err.kind() != ErrorKind::NotFound {
            return Err(err)
        }

        fs::create_dir_all(&config_dir)?;
        File::create(&file_path)?;
    }

    Ok(file_path)
}

#[cfg(test)]
mod goto_test {
    use super::*;

    // #[test]
    // fn test_name() {
    //     let mut app = App::default();
    //     add_target_dir(&mut app, 'a', "/aaa/bbb/ccc".to_owned())
    //         .unwrap();
    //     println!("{:?}", app.target_dir);
    //     panic!()
    // }

    // #[test]
    // fn test_b() {
    //     let mut app = App::default();
    //     remove_target_dir(&mut app, 'a').unwrap();
    // }

    #[test]
    fn test_print() {
        let mut app = App::default();
        init_config(&mut app).unwrap();
        // println!("{}\n{:?}", app.config_path, app.target_dir);
        // panic!()
    }

    #[test]
    fn test_truncate() {
        let a = "\n".to_owned();
        assert_eq!("", a.trim());
    }

}
