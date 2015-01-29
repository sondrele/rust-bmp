rust-bmp
========
[![Build Status](https://travis-ci.org/sondrele/rust-bmp.svg?branch=master)](https://travis-ci.org/sondrele/rust-bmp)

Small module for reading and writing bitmap images.
Currently only 24-bit RGB BMP images are supported.

Usage
-----
Initialize a new image with the `new` function, by specifying `width` and `height`.
```rust
extern crate bmp;
use bmp::Image;

let mut img = Image::new(100, 100);
```
Edit image data using the `get_pixel` and `set_pixel` functions.
Save an image with the `save` function, by specifying the `path`.
```rust
let pixel = img.get_pixel(0, 0);
img.set_pixel(50, 50, Pixel{r: 255, g: 255, b: 255});
img.save("path/to/img.bmp");
```
Open an existing image with the `open` function, by specifying the `path`.
```rust
let mut img = Image::open("path/to/img.bmp");
```
Coordinate convention
---------------------
The BMP images are accessed in row-major order, where point (0, 0) is defined to  be in the
upper left corner of the image.
Example
-------
```rust
extern crate bmp;

use bmp::Image;

fn main() {
    let mut img = Image::new(256, 256);

    for (x, y) in img.coordinates() {
        img.set_pixel(x, y, bmp::Pixel {
            r: (x - y / 256) as u8,
            g: (y - x / 256) as u8,
            b: (x + y / 256) as u8
        })
    }
    img.save("img.bmp");
}

```
