// Edit mode

mod types;

use std::ops::Range;

use ratatui::widgets::ListState;

use crate::{app::CursorPos, error::AppResult, rt_error, utils::{get_window_height, Direction}};

use super::{cursor_movement::move_cursor_core, Goto};

pub use types::*;

// NOTE: The Edit mode can only be used for current_block.
// And it's not allow to enable it in root directory of Linux.
impl<'a> EditMode<'a> {
    pub fn item_navigation(
        &mut self,
        state: &mut ListState,
        direction: Goto,
        mark_expand: bool
    ) -> AppResult<()> {
        if self.items.is_empty() || state.selected().is_none() {
            return Ok(())
        }

        let expand_region = move_cursor_core(
            direction,
            state,
            self.items.len(),
            mark_expand
        );

        if let Some(range) = expand_region {
            for i in range {
                self.items[i].mark = true;
            }
        }

        Ok(())
    }

    pub fn cursor_move(
        &mut self,
        direction: Direction,
        edge: bool
    ) -> AppResult<()> {
        for item in self.items.iter_mut() {
            match direction {
                Direction::Left => {
                    match item.cursor {
                        CursorPos::Index(ref mut idx) => {
                            if edge {
                                item.cursor = CursorPos::Index(0);
                            } else {
                                if *idx > 0 {
                                    *idx -= 1;
                                }
                            }
                        },
                        CursorPos::End => {
                            if edge {
                                item.cursor = CursorPos::Index(0);
                            } else {
                                item.cursor = CursorPos::Index(
                                    item.editing_name.len() - 1
                                );
                            }
                        },
                        CursorPos::None => ()
                    }
                },

                Direction::Right => {
                    match item.cursor {
                        CursorPos::Index(ref mut index) => {
                            if edge {
                                item.cursor = CursorPos::End;
                            } else {
                                if *index < item.editing_name.len() - 1 {
                                    *index += 1;
                                    continue;
                                }

                                if *index == item.editing_name.len() - 1 {
                                    item.cursor = CursorPos::End;
                                }
                            }
                        },
                        _ => ()
                    }
                },
                _ => rt_error!("Wrong direction to cursor movement")
            }
        }

        Ok(())
    }

    /// Enter insert modal.
    pub fn insert(&mut self, state: &mut ListState, pos: CursorPos) {
        if self.items.is_empty() {
            return ()
        }

        let mut really_insert = false;
        for item in self.items.iter_mut() {
            if item.mark {
                if !really_insert {
                    really_insert = true;
                }

                item.cursor = pos;
            }
        }

        if let Some(selected_item) = state.selected() {
            let item = &mut self.items[selected_item];
            if !item.mark {
                if !really_insert {
                    really_insert = true;
                }

                item.cursor = pos;
            }
        }

        if really_insert {
            self.insert = true;
        }
    }

    // pub fn mark_delete(&mut self, ) -> Type {
        
    // }
}
