// Utils for crate.

use clap::Parser;
use anyhow::bail;

#[derive(Parser)]
pub struct Args {
    /// Directly spawn a shell in the working directory
    #[arg(
        short,
        long,
        default_value_t = false,
        conflicts_with = "start_from"
    )]
    pub working_directory: bool,

    #[arg(
        short,
        long,
        default_value_t = String::from("NULL"),
        conflicts_with = "working_directory"
    )]
    pub start_from: String,

    /// The target file for output, require a absolute path
    #[arg(short, long, default_value_t = String::from("NULL"))]
    pub output_file: String,
}

#[derive(Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right
}

impl Direction {
    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        Ok(
            match value {
                "up" => Self::Up,
                "down" => Self::Down,
                "left" => Self::Left,
                "right" => Self::Right,
                _ => bail!("Unknow keyword to parse into Direction!")
            }
        )
    }
}

/// Split String into Vec<String>
pub fn str_split(_string: String) -> Vec<String> {
    let mut str_vec: Vec<String> = Vec::new();

    for _str in _string.split(" ") {
        str_vec.push(_str.to_owned());
    }

    str_vec
}
