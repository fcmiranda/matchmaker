use ratatui_image::{picker::Picker, protocol::Protocol, Resize};
fn main() {
    let mut picker = Picker::from_termios().unwrap();
    let img = image::DynamicImage::new_rgba8(10, 10);
    let p = picker.new_resize_protocol(img);
}
