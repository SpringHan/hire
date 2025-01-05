// Simple operations.

use super::App;
use super::switch_operation::SwitchCase;

pub fn print_full_path(app: &mut App) {
    let file_name = if let Some(file_saver) = app.get_file_saver() {
        file_saver.name.to_owned()
    } else {
        String::new()
    };

    let mut full_path: String = app.path.to_string_lossy().into();

    full_path = if full_path == "/" {
        format!("/{}", file_name)
    } else {
        format!("{}/{}", full_path, file_name)
    };

    SwitchCase::new(app, |_, _, _| Ok(()), full_path, None::<bool>)
}
