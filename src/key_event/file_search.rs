// File Search

use std::{sync::mpsc::{self, Receiver, Sender}, thread};

use anyhow::bail;

use crate::{app::{Block, FileSaver}, error::AppResult, key_event::Goto};

use super::App;

#[derive(Default)]
pub struct FileSearcher {
    searched_idx: Vec<usize>,
    calc_sender: Option<Sender<(String, Vec<FileSaver>)>>
}

impl FileSearcher {
    pub fn update_idx(&mut self, idx_set: Vec<usize>) {
        self.searched_idx = idx_set;
    }
}

impl<'a> App<'a> {
    pub fn init_search_channel(&mut self) -> Receiver<Vec<usize>> {
        let (update_tx, update_rx) = mpsc::channel::<Vec<usize>>();
        let (calc_tx, calc_rx)     = mpsc::channel::<(String, Vec<FileSaver>)>();

        // let thread_update_tx = update_tx.to_owned();
        thread::spawn(move || loop {
            if let Ok((name, files)) = calc_rx.recv() {
                let name = name.to_lowercase();

                let mut i = 0;
                let mut indexes: Vec<usize> = Vec::new();
                for file in files.iter() {
                    if file.name.to_lowercase().contains(&name) {
                        indexes.push(i);
                    }
                    i += 1;
                }

                update_tx.send(indexes)
                    .expect("Error occurred from channel when searching file!")
            }
        });

        self.file_searcher.calc_sender = Some(calc_tx);

        update_rx
    }

    pub fn file_search(&mut self, name: String) -> anyhow::Result<()> {
        self.command_history.push(format!("/{}", name.clone()));

        let current_files = self.get_directory_mut().0.clone();
        if let Some(ref sender) = self.file_searcher.calc_sender {
            if let Err(err) = sender.send((name, current_files)) {
                return Err(err.into())
            }

            return Ok(())
        }

        bail!("Cannot find sender in file_searcher")
    }

    pub fn prev_candidate(&mut self) -> AppResult<()> {
        self.move_candidate(false)?;

        Ok(())
    }

    pub fn next_candidate(&mut self) -> AppResult<()> {
        self.move_candidate(true)?;

        Ok(())
    }

    /// Move current cursor to next/previous searched file name.
    /// When NEXT is true, searching the next. Otherwise the previous.
    fn move_candidate(&mut self,
                      next: bool
    ) -> AppResult<()>
    {
        use crate::key_event::move_cursor;

        let candidates = &self.file_searcher.searched_idx;

        let in_root = if let Block::Browser(true) = self.selected_block {
            true
        } else {
            false
        };

        let current_idx = if in_root {
            self.selected_item.parent.selected().unwrap()
        } else {
            self.selected_item.current.selected().unwrap()
        };

        let target = if next {
            get_search_index(candidates.iter(), current_idx, true)
        } else {
            get_search_index(candidates.iter().rev(), current_idx, false)
        };

        if let Some(idx) = target {
            move_cursor(self, Goto::Index(idx), in_root)?;
        }

        Ok(())
    }
    
    pub fn clean_search_idx(&mut self) {
        self.file_searcher.searched_idx.clear();
    }
}

#[inline]
fn get_search_index<'a, T>(iter: T,
                           current: usize,
                           next: bool
) -> Option<usize>
where T: Iterator<Item = &'a usize>
{
    let mut get_current_idx = false;

    for i in iter {
        if get_current_idx {
            return Some(*i)
        }

        if !next && *i < current {
            return Some(*i)
        }

        if next && *i > current {
            return Some(*i)
        }

        if *i == current {
            get_current_idx = true;
            continue;
        }
    }

    None
}
