// Types for cursor movement.

/// The enum that used to control move of `List` widget in ratatui.
#[derive(PartialEq, Eq)]
pub enum Goto {
    Up,
    Down,
    ScrollUp,
    ScrollDown,
    Index(usize),
}

#[derive(Default)]
pub struct NaviIndex {
    show: bool,
    inputing: Option<usize>,
}

impl NaviIndex {
    pub fn show(&self) -> bool {
        self.show
    }

    pub fn init(&mut self) {
        self.show = true;
        self.inputing = None;
    }

    pub fn input(&mut self, number: u8) {
        if let Some(ref mut _num) = self.inputing {
            *_num = *_num * 10 + number as usize;
        } else {
            self.inputing = Some(number as usize);
        }
    }

    pub fn backspace(&mut self) {
        if let Some(ref mut _num) = self.inputing {
            if *_num < 10 {
                self.inputing = None;
                return ()
            }

            *_num /= 10;
        }
    }

    pub fn index(&self) -> Option<usize> {
        self.inputing
    }

    pub fn reset(&mut self) {
        self.show = false;
        self.inputing = None;
    }
}
