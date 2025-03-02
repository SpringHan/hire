// Image Preview

use std::{
    thread,
    path::{Path, PathBuf},
    sync::{
        Arc,
        Mutex,
        mpsc::{self, Receiver, Sender},
    },
};

use image::DynamicImage;
use ratatui::layout::Rect;
use anyhow::{bail, Result};
use ratatui_image::{
    Resize,
    picker::Picker,
    thread::ThreadProtocol,
    protocol::StatefulProtocol,
};

use super::App;

pub type ResizeRequest = (StatefulProtocol, Resize, Rect);

#[derive(Default)]
pub struct ImagePreview {
    picker: Option<Picker>,
    protocol: Option<ThreadProtocol>,
    image_path: Arc<Mutex<(PathBuf, bool)>>,
    resize_sender: Option<Sender<ResizeRequest>>,
}

impl ImagePreview {
    /// Check whether the current terminal supports image preview feature.
    pub fn with_image_feat(&self) -> bool {
        self.picker.is_some()
    }

    pub fn make_protocol(&mut self, image: DynamicImage) -> Result<()> {
        if self.picker.is_none() {
            bail!("Unable to setup image picker");
        }

        let rtx: Sender<ResizeRequest>;
        if let Some(ref sender) = self.resize_sender {
            rtx = sender.to_owned();
        } else {
            bail!("Unable to init image protocol thread");
        }

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

    pub fn send_path(&self, path: PathBuf) -> Result<()> {
        if let Ok(mut _mutex) = self.image_path.lock() {
            *_mutex = (path, true);

            return Ok(())
        }

        bail!("Failed to send path for decoding image thread")
    }
}

impl App {
    pub fn init_image_picker(
        &mut self
    ) -> Option<(Receiver<StatefulProtocol>, Receiver<Option<DynamicImage>>)>
    {
        let preview = &mut self.image_preview;

        let picker = Picker::from_query_stdio();
        if picker.is_err() {
            return None
        }

        preview.picker = Some(picker.unwrap());

        let (resize_tx, resize_rx) = mpsc::channel::<ResizeRequest>();
        let (prot_tx, prot_rx)     = mpsc::channel::<StatefulProtocol>();
        let (image_tx, image_rx)   = mpsc::channel::<Option<DynamicImage>>();

        // Image resize thread
        thread::spawn(move || loop {
            if let Ok((mut protocol, resize, area)) = resize_rx.recv() {
                protocol.resize_encode(&resize, protocol.background_color(), area);

                if prot_tx.send(protocol).is_err() {
                    break;
                }
            }
        });

        // Image decode thread
        let path_ref = Arc::clone(&preview.image_path);
        thread::spawn(move || {
            let mut current_path = PathBuf::new();
            let mut decode_result: Option<DynamicImage>;

            loop {
                if let Ok(mut _ref) = path_ref.try_lock() {
                    if current_path == _ref.0 && !_ref.1 {
                        continue;
                    }
                    _ref.1 = false;
                    current_path = _ref.0.to_owned();
                } else {
                    continue;
                }

                if current_path.to_string_lossy() != "" {
                    // Calculate DynamicImage
                    if let Ok(image_data) = get_image_info(current_path.to_owned()) {
                        decode_result = image_data;
                    } else {
                        decode_result = None;
                    }

                    // Try to send DynamicImage to channel
                    if let Ok(_ref) = path_ref.try_lock() {
                        if _ref.0 == current_path {
                            image_tx.send(decode_result)
                                .expect("Failed to send DynamicImage within channel!");
                        }
                    }
                }
            }
        });

        preview.resize_sender = Some(resize_tx);

        Some((prot_rx, image_rx))
    }
}

pub fn get_image_info<P: AsRef<Path>>(path: P) -> Result<Option<DynamicImage>> {
    let img = image::ImageReader::open(path)?.decode();

    if img.is_err() {
        return Ok(None)
    }

    Ok(Some(img.unwrap()))
}
