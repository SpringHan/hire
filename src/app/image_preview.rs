// Image Preview

use std::{
    path::Path,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
        Mutex, MutexGuard
    },
    thread
};

use image::DynamicImage;
use anyhow::{bail, Context, Result};
use ratatui::layout::Rect;
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, thread::ThreadProtocol, Resize};

use super::App;

#[derive(Default)]
pub struct ImagePreview {
    picker: Option<Picker>,
    // protocol: Arc<Mutex<Option<ThreadProtocol>>>,
    protocol: Option<ThreadProtocol>,
    protocol_sender: Option<Sender<StatefulProtocol>>,
}

impl ImagePreview {
    pub fn make_protocol(&mut self, image: DynamicImage) -> Result<()> {
        if self.picker.is_none() {
            bail!("Unable to setup image picker");
        }

        let ptx: Sender<StatefulProtocol>;
        if let Some(ref sender) = self.protocol_sender {
            ptx = sender.to_owned();
        } else {
            bail!("Unable to init image protocol thread");
        }

        let (rtx, rrx) = mpsc::channel::<(StatefulProtocol, Resize, Rect)>();
        thread::spawn(move || loop {
            if let Ok((mut protocol, resize, area)) = rrx.recv() {
                protocol.resize_encode(&resize, protocol.background_color(), area);

                if ptx.send(protocol).is_err() {
                    break;
                }
            }
        });

        // if let Ok(mut protocol_ref) = self.protocol.lock() {
        //     *protocol_ref = Some(ThreadProtocol::new(
        //         rtx,
        //         self.picker.unwrap().new_resize_protocol(image)
        //     ));
        // } else {
        //     bail!("Failed to init image protocol")
        // }

        self.protocol = Some(ThreadProtocol::new(
            rtx,
            self.picker.unwrap().new_resize_protocol(image)
        ));

        Ok(())
    }

    pub fn image_protocol(&mut self) -> Option<&mut ThreadProtocol> {
        match self.protocol {
            Some(ref mut _ref) => Some(_ref),
            None => None,
        }
    }
}

impl App {
    pub fn init_image_picker(&mut self) -> Option<Receiver<StatefulProtocol>> {
        let preview = &mut self.image_preview;

        let picker = Picker::from_query_stdio();
        if picker.is_err() {
            return None
        }

        preview.picker = Some(picker.unwrap());

        let (ptx, prx) = mpsc::channel::<StatefulProtocol>();
        preview.protocol_sender = Some(ptx);

        Some(prx)
    }
}

pub fn get_image_info<P: AsRef<Path>>(path: P) -> Result<Option<DynamicImage>> {
    let img = image::ImageReader::open(path)?.decode();

    if img.is_err() {
        return Ok(None)
    }

    Ok(Some(img.unwrap()))
}
