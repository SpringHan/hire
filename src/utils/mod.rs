// Utils for crate.

use anyhow::bail;

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
