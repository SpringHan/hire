// Image Preview

use std::{
    thread,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
};

use image::DynamicImage;
use anyhow::{anyhow, bail, Result};
use ratatui_image::{
    thread::{ResizeRequest, ResizeResponse, ThreadProtocol},
    errors::Errors,
    picker::Picker,
};

use crate::utils::file_type_label;

use super::App;

pub type Response = Result<ResizeResponse, Errors>;

/// The path the preview thread was asked for, and what to show for it.
pub type PreviewUpdate = (PathBuf, Preview);

/// What the preview thread produced for one file.
#[derive(Debug)]
pub enum Preview {
    /// The file decoded as an image.
    Image(DynamicImage),

    /// The file is not an image: what to show instead, its MIME type.
    FileType(String),
}

#[derive(Default)]
pub struct ImagePreview {
    pub useless: bool,
    picker: Option<Picker>,
    protocol: Option<ThreadProtocol>,
    /// Paths to preview are handed to the preview thread through this channel.
    path_sender: Option<Sender<PathBuf>>,
    resize_sender: Option<Sender<ResizeRequest>>,
}

impl ImagePreview {
    /// Check whether the current terminal supports image preview feature.
    pub fn with_image_feat(&self) -> bool {
        self.picker.is_some()
    }

    /// The image protocol chosen for the real terminal, if it supports images.
    pub fn protocol_type(&self) -> Option<ratatui_image::picker::ProtocolType> {
        self.picker.as_ref().map(|picker| picker.protocol_type())
    }

    /// The size of one terminal cell in pixels `(width, height)`.
    pub fn font_size(&self) -> Option<(u16, u16)> {
        self.picker.as_ref().map(|picker| picker.font_size())
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
            Some(self.picker.as_ref().unwrap().new_resize_protocol(image))
        ));

        Ok(())
    }

    pub fn image_protocol(&mut self) -> Option<&mut ThreadProtocol> {
        match self.protocol {
            Some(ref mut _ref) => Some(_ref),
            None => None,
        }
    }

    /// Ask the preview thread for `path`.
    pub fn send_path(&self, path: PathBuf) -> Result<()> {
        let Some(sender) = self.path_sender.as_ref() else {
            bail!("The preview thread is not running")
        };

        sender
            .send(path)
            .map_err(|err| anyhow!("Failed to send the path to the preview thread: {err}"))
    }
}

impl<'a> App<'a> {
    pub fn init_image_picker(
        &mut self
    ) -> Option<(Receiver<Response>, Receiver<PreviewUpdate>)>
    {
        let preview = &mut self.image_preview;

        let picker = Picker::from_query_stdio();
        if picker.is_err() {
            return None
        }

        preview.picker = Some(picker.unwrap());

        let (prot_tx, prot_rx)     = mpsc::channel::<Response>();
        let (resize_tx, resize_rx) = mpsc::channel::<ResizeRequest>();
        let (image_tx, image_rx)   = mpsc::channel::<PreviewUpdate>();
        let (path_tx, path_rx)     = mpsc::channel::<PathBuf>();

        // Image resize thread
        thread::spawn(move || {
            while let Ok(request) = resize_rx.recv() {
                if prot_tx.send(request.resize_encode()).is_err() {
                    break;
                }
            }
        });

        // Preview thread: decode the requested path, or describe its type when
        // it is not an image. `recv` blocks while nothing is asked for, so the
        // thread costs nothing between previews.
        thread::spawn(move || {
            while let Ok(mut path) = path_rx.recv() {
                // Only the newest request matters: browsing a directory must
                // not queue one decode per file.
                while let Ok(newer) = path_rx.try_recv() {
                    path = newer;
                }

                let preview = match get_image_info(&path) {
                    Ok(Some(image)) => Preview::Image(image),
                    // Not an image, or it cannot be read: show its type.
                    _ => Preview::FileType(file_type_label(&path)),
                };

                if image_tx.send((path, preview)).is_err() {
                    break
                }
            }
        });

        preview.path_sender = Some(path_tx);
        preview.useless = true;
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
