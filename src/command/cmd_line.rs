// Command Line

use std::ops::{AddAssign, SubAssign};

use crate::{
    app::{Block, CursorPos, FileOperation, OptionFor},
    error::{AppResult, ErrorType},
    key_event::Goto,
    App,
};

impl App {
    pub fn set_command_line<T: Into<String>>(&mut self, content: T, pos: CursorPos) {
        self.selected_block = Block::CommandLine(content.into(), pos);
    }

    pub fn command_line_append(&mut self, content: char) {
        if let
            Block::CommandLine(
                ref mut origin,
                ref mut cursor
            ) = self.selected_block
        {
            if let CursorPos::Index(idx) = cursor {
                origin.insert(*idx, content);
                idx.add_assign(1);
                return ()
            }

            origin.push(content);
        }
    }
    
    /// Quit command line selection.
    pub fn quit_command_mode(&mut self) {
        self.selected_block = self::Block::Browser(self.root());

        if self.command_idx.is_some() {
            self.command_idx = None;
        }

        if self.command_expand {
            self.command_expand = false;
            self.command_scroll = None;
        }

        if self.command_error {
            self.command_error = false;
        }

        self.option_key = OptionFor::None;
    }
    
    /// The function will change content in command line.
    /// In the meanwhile, adjusting current command index.
    pub fn command_select(&mut self, direct: Goto) {
        if let
            Block::CommandLine(
                ref mut current,
                ref mut cursor
            ) = self.selected_block
        {
            if self.command_history.is_empty() {
                return ()
            }

            if *cursor != CursorPos::End {
                *cursor = CursorPos::End;
            }

            if let Some(index) = self.command_idx {
                match direct {
                    Goto::Up => {
                        if index == 0 {
                            return ()
                        }
                        self.command_idx = Some(index - 1);
                        *current = self.command_history[index - 1].to_owned();
                    },
                    Goto::Down => {
                        if index == self.command_history.len() - 1 {
                            return ()
                        }
                        self.command_idx = Some(index + 1);
                        *current = self.command_history[index + 1].to_owned()
                    },
                    _ => panic!("Unvalid value!")
                }
                return ()
            }

            // Initial selection.
            let current_idx = match direct {
                Goto::Up => {
                    self.command_history
                        .iter()
                        .rev()
                        .position(|x| x == current)
                },
                Goto::Down => {
                    // The command search function can only be executed by UP key.
                    return ()
                },
                _ => panic!("Unvalid value!")
            };
            if let Some(idx) = current_idx {
                if direct == Goto::Up {
                    // The real idx is: len - 1 - IDX
                    if idx == self.command_history.len() - 1 {
                        self.command_idx = Some(0);
                        return ()
                    }
                    let temp_idx = self.command_history.len() - 2 - idx;
                    self.command_idx = Some(temp_idx);
                    *current = self.command_history[temp_idx].to_owned();
                } else {
                    if idx + 1 == self.command_history.len() {
                        self.command_idx = Some(idx);
                        return ()
                    }
                    self.command_idx = Some(idx + 1);
                    *current = self.command_history[idx + 1].to_owned();
                }
            } else {
                if direct == Goto::Up {
                    self.command_idx = Some(self.command_history.len() - 1);
                    *current = self.command_history.last().unwrap().to_owned();
                } else {
                    self.command_idx = Some(0);
                    *current = self.command_history.first().unwrap().to_owned();
                }
            }
        }
    }
    
    pub fn command_parse(&mut self) -> AppResult<()> {
        if let Block::CommandLine(ref command, _) = self.selected_block {
            if command.starts_with("/") {
                self.file_search(command[1..].to_owned());
                return Ok(self.quit_command_mode())
            }

            self.command_history.push(command.to_owned());
            let mut command_slices: Vec<&str> = command.split(" ").collect();

            match command_slices[0] {
                ":rename" => {
                    command_slices.remove(0);
                    let file_name = command_slices.join(" ");
                    super::cmds::rename_file(
                        self.path.to_owned(),
                        self,
                        file_name
                    )?
                },
                ":create_file" => {
                    command_slices.remove(0);
                    let files = command_slices.join(" ");
                    let files: Vec<&str> = files.split(",").collect();
                    super::cmds::create_file(
                        self,
                        files.into_iter(),
                        false
                    )?
                },
                ":create_dir" => {
                    command_slices.remove(0);
                    let files = command_slices.join(" ");
                    let files: Vec<&str> = files.split(",").collect();
                    super::cmds::create_file(
                        self,
                        files.into_iter(),
                        true
                    )?
                },
                ":create_symlink" => {
                    self.marked_files.clear();
                    self.marked_operation = FileOperation::None;

                    command_slices.remove(0);
                    let files = command_slices.join(" ");
                    let files: Vec<&str> = files
                        .split("->")
                        .collect();
                    super::cmds::create_symlink(
                        self,
                        [(files[0].trim(), files[1].trim())].into_iter()
                    )?
                },
                _ => return Err(ErrorType::UnvalidCommand.pack())
            }

            self.quit_command_mode();
        }

        Ok(())
    }

    pub fn expand_init(&mut self) {
        self.command_expand = true;
        self.command_scroll = Some(0);
    }

    pub fn expand_scroll(&mut self, direct: Goto) {
        if let Some(ref mut scroll) = self.command_scroll {
            match direct {
                Goto::Up => {
                    if *scroll > 0 {
                        scroll.sub_assign(1);
                    }
                },
                Goto::Down => scroll.add_assign(1),
                _ => panic!("Unknown error!")
            }
        }
    }
}
