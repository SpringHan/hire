// Image Preview

use ratatui_image::{picker::Picker, protocol::StatefulProtocol};

use crate::error::AppResult;

pub struct ImagePreview {
    picker: Picker,
    protocol: Option<StatefulProtocol>
}

impl ImagePreview {
    // pub fn new() -> AppResult<Self> {
    //     Ok(
    //         Self {
    //             picker: Picker::from_query_stdio(),
    //             protocol: None
    //         }
    //     )
    // }

}
