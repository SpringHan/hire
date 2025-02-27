// Image Preview

use std::path::Path;

use image::DynamicImage;
use anyhow::{bail, Result};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};

use super::App;

#[derive(Default)]
pub struct ImagePreview {
    picker: Option<Picker>,
    protocol: Option<StatefulProtocol>
}

impl ImagePreview {
    pub fn make_protocol(&mut self, image: DynamicImage) -> Result<()> {
        if self.picker.is_none() {
            bail!("Unable to setup image picker");
        }

        self.protocol = Some(
            self.picker.unwrap().new_resize_protocol(image)
        );

        Ok(())
    }

    pub fn image_protocol(&mut self) -> Option<&mut StatefulProtocol> {
        match self.protocol {
            Some(ref mut protocol) => Some(protocol),
            None => None,
        }
    }
}

impl App {
    pub fn init_image_picker(&mut self) {
        let preview = &mut self.image_preview;

        let picker = Picker::from_query_stdio();

        if picker.is_err() {
            return ()
        }

        preview.picker = Some(picker.unwrap());
    }
}

pub fn get_image_info<P: AsRef<Path>>(path: P) -> Result<Option<DynamicImage>> {
    let img = image::ImageReader::open(path)?.decode();

    if img.is_err() {
        return Ok(None)
    }

    Ok(Some(img.unwrap()))
}
