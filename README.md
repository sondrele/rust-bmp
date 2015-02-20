rust-bmp
========
[![Build Status](https://travis-ci.org/sondrele/rust-bmp.svg?branch=master)](https://travis-ci.org/sondrele/rust-bmp)

[Full documentation](http://sondrele.github.io/rust-bmp/bmp/index.html)

Small module for reading and writing bitmap images.
Currently only 24-bit RGB BMP images are supported.

Usage
-----
The library should be available on [crates.io](https://crates.io/crates/bmp),
but updated versions of the crate might lag behind until 1.0.0 of Rust has been released.

To ensure that the crate is up to date, add it as a git dependency to `Cargo.toml` in your project.
```toml
[dependencies.bmp]
git = "https://github.com/sondrele/rust-bmp"
```
###Initializing
Initialize a new image with the `new` function, by specifying `width` and `height`.
```rust
extern crate bmp;
use bmp::Image;

let mut img = Image::new(100, 100);
```
###Editing
Edit image data using the `get_pixel` and `set_pixel` functions.
Save an image with the `save` function, by specifying the `path`. The function returns
an `IoResult` which indicates whether the save was successful or not.
```rust
let pixel = img.get_pixel(0, 0);
img.set_pixel(50, 50, Pixel { r: 255, g: 255, b: 255 });
let _ = img.save("path/to/img.bmp");
```
###Opening
Open an existing image with the `open` function, by specifying the `path`. The function
returns a `BmpResult`, that contains either a `Image` or a `BmpError`.
```rust
let mut img = match Image::open("path/to/img.bmp") {
    Ok(img) => img,
    Err(e) => panic!("{}", e)
};
// or ...
let mut img = Image::open("path/to/img.bmp").unwrap();
```
Coordinate convention
---------------------
The BMP images are accessed in row-major order, where point (0, 0) is defined to  be in the
upper left corner of the image.
Example
-------
```rust
extern crate bmp;

use bmp::{Image, Pixel};

fn main() {
    let mut img = Image::new(256, 256);

    for (x, y) in img.coordinates() {
        img.set_pixel(x, y, Pixel {
            r: (x - y / 256) as u8,
            g: (y - x / 256) as u8,
            b: (x + y / 256) as u8
        })
    }
    let _ = img.save("img.bmp");
}

```
