#[macro_use]
extern crate bmp;
use bmp::{Image, RGB};

fn main() {
    let mut img = Image::new(256, 256);

    for (x, y) in img.coordinates() {
        img.set_pixel(x, y, px!(x, y, 200));
    }
    let _ = img.save("img.bmp");
}
