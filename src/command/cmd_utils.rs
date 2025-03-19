// Command Line

use std::{io::Stderr, ops::AddAssign};

use ratatui::{prelude::CrosstermBackend, Terminal};

use crate::{
    app::{Block, CursorPos, FileOperation, OptionFor},
    key_event::{CommandStr, Goto, ShellCommand},
    error::{AppResult, ErrorType},
    utils::Direction,
    rt_error,
    App
};

impl Block {
    pub fn set_command_line<T: Into<String>>(&mut self, content: T, pos: CursorPos) {
        *self = Block::CommandLine(content.into(), pos);
    }
}

impl<'a> App<'a> {
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

            if !self.command_completion.popup_info().0.is_empty() {
                self.command_completion.reset();
            }
        }
    }
    
    /// Quit command line selection.
    pub fn quit_command_mode(&mut self) {
        self.selected_block = self::Block::Browser(self.root());

        if self.command_idx.is_some() {
            self.command_idx = None;
        }

        if self.command_expand {
            self.expand_quit();
        }

        if self.command_error {
            self.command_error = false;
        }

        if self.command_warning {
            self.command_warning = false;
        }

        if !self.command_completion.popup_info().0.is_empty() {
            self.command_completion.reset();
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
    
    pub fn command_parse(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stderr>>
    ) -> AppResult<()> {
        if let Block::CommandLine(ref _command, _) = self.selected_block {
            if _command.starts_with("/") {
                self.file_search(_command[1..].to_owned(), false)?;
                return Ok(self.quit_command_mode())
            }

            let argu_err = "Wrong number argument for current command";

            let command = _command.to_owned();
            self.command_history.push(command.to_owned());
            let mut command_slices: Vec<&str> = command.split(" ").collect();

            match command_slices[0] {
                ":rename" => {
                    if command_slices.len() < 2 {
                        rt_error!("{argu_err}")
                    }

                    command_slices.remove(0);
                    let file_name = command_slices.join(" ");
                    super::cmds::rename_file(
                        self.path.to_owned(),
                        self,
                        file_name
                    )?
                },

                ":create_file" => {
                    if command_slices.len() < 2 {
                        rt_error!("{argu_err}")
                    }

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
                    if command_slices.len() < 2 {
                        rt_error!("{argu_err}")
                    }

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
                    if command_slices.len() != 4 {
                        rt_error!("{argu_err}")
                    }

                    self.marked_files.clear();
                    self.marked_operation = FileOperation::None;

                    command_slices.remove(0);
                    let files = command_slices;
                    // let files: Vec<&str> = files
                    //     .split("->")
                    //     .collect();
                    super::cmds::create_symlink(
                        self,
                        [(files[0].trim(), files[1].trim())].into_iter()
                    )?
                },

                // Shell command
                shell if shell.starts_with(":!") => {
                    if command_slices.len() < 2 {
                        rt_error!("{argu_err}")
                    }

                    command_slices.remove(0);
                    let shell_program = &shell[2..];

                    crate::key_event::shell_process(
                        self,
                        terminal,
                        ShellCommand::Command(
                            Some(shell_program),
                            CommandStr::from_strs(command_slices)
                        ),
                        true
                    )?;
                },
                _ => return Err(ErrorType::UnvalidCommand.pack())
            }

            self.quit_command_mode();
        }

        Ok(())
    }

    pub fn expand_init(&mut self) {
        self.command_expand = true;
        self.command_scroll = Some((0, 0));
    }

    pub fn expand_quit(&mut self) {
        self.command_expand = false;
        self.command_scroll = None;
    }

    pub fn expand_scroll(&mut self, direct: Direction) {
        if let Some(ref mut scroll) = self.command_scroll {
            match direct {
                Direction::Left => {
                    if scroll.1 > 0 {
                        scroll.1 -= 1;
                    }
                },
                Direction::Right => scroll.1 += 1,
                Direction::Up => {
                    if scroll.0 > 0 {
                        scroll.0 -= 1;
                    }
                },
                Direction::Down => scroll.0 += 1,
            }
        }
    }
}
