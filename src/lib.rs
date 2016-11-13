#![warn(warnings)]
#![cfg_attr(test, deny(warnings))]

//! A small library for reading and writing BMP images.
//!
//! The library supports uncompressed BMP Version 3 images.
//! The different decoding and encoding schemes is shown in the table below.
//!
//! |Scheme | Decoding | Encoding | Compression |
//! |-------|----------|----------|-------------|
//! | 24 bpp| ✓        | ✓        | No          |
//! | 8 bpp | ✓        | ✗        | No          |
//! | 4 bpp | ✓        | ✗        | No          |
//! | 1 bpp | ✓        | ✗        | No          |
//!
//! # Example
//!
//! ```
//! #[macro_use]
//! extern crate bmp;
//! use bmp::{Image, Pixel};
//!
//! fn main() {
//!     let mut img = Image::new(256, 256);
//!
//!     for (x, y) in img.coordinates() {
//!         img.set_pixel(x, y, px!(x, y, 200));
//!     }
//!     let _ = img.save("img.bmp");
//! }
//! ```
//!

extern crate byteorder;

use std::convert::{AsRef};
use std::fmt;
use std::fs;
use std::io;
use std::io::{Cursor, Read, Write};
use std::iter::Iterator;

use ::CompressionType::*;
use ::BmpVersion::*;

pub use decoder::{BmpError, BmpErrorKind, BmpResult};

#[cfg(test)]
mod tests;

/// The pixel data used in the `Image`.
///
/// It has three values for the `red`, `blue` and `green` color channels, respectively.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8
}

impl Pixel {
    pub fn new(r: u8, g: u8, b: u8) -> Pixel {
        Pixel { r: r, g: g, b: b }
    }
}

/// Macro to generate a `Pixel` from `r`, `g` and `b` values.
#[macro_export]
macro_rules! px {
    ($r:expr, $g:expr, $b:expr) => {
        Pixel { r: $r as u8, g: $g as u8, b: $b as u8 }
    }
}

macro_rules! file_size {
    ($bpp:expr, $width:expr, $height:expr) => {{
        let header_size = 2 + 12 + 40;
        let row_size = (($bpp as f32 * $width as f32 + 31.0) / 32.0).floor() as u32 * 4;
        (header_size as u32, $height as u32 * row_size)
    }}
}

/// Common color constants accessible by names.
pub mod consts;

mod decoder;
mod encoder;

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
enum BmpVersion {
    Version1,
    Version2,
    Version3,
    Version3NT,
    Version4,
}

impl AsRef<str> for BmpVersion {
    fn as_ref(&self) -> &str {
        match *self {
            Version1   => "BMP Version 1",
            Version2   => "BMP Version 2",
            Version3   => "BMP Version 3",
            Version3NT => "BMP Version 3 NT",
            Version4   => "BMP Version 4",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CompressionType {
    Uncompressed,
    Rle8bit,
    Rle4bit,
    // Only for BMP version 4
    BitfieldsEncoding,
}

impl CompressionType {
    fn from_u32(val: u32) -> CompressionType {
        match val {
            1 => Rle8bit,
            2 => Rle4bit,
            3 => BitfieldsEncoding,
            _ => Uncompressed,
        }
    }
}

impl AsRef<str> for CompressionType {
    fn as_ref(&self) -> &str {
        match *self {
            Rle8bit           => "RLE 8-bit",
            Rle4bit           => "RLE 4-bit",
            BitfieldsEncoding => "Bitfields Encoding",
            Uncompressed      => "Uncompressed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BmpHeader {
    file_size: u32,
    creator1: u16,
    creator2: u16,
    pixel_offset: u32
}

impl BmpHeader {
    pub fn new(header_size: u32, data_size: u32) -> BmpHeader {
        BmpHeader {
            file_size: header_size + data_size,
            creator1: 0 /* Unused */,
            creator2: 0 /* Unused */,
            pixel_offset: header_size
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BmpDibHeader {
    header_size: u32,
    width: i32,
    height: i32,
    num_planes: u16,
    bits_per_pixel: u16,
    compress_type: u32,
    data_size: u32,
    hres: i32,
    vres: i32,
    num_colors: u32,
    num_imp_colors: u32,
}

impl BmpDibHeader {
    pub fn new(width: i32, height: i32) -> BmpDibHeader {
        let row_size = ((24.0 * width as f32 + 31.0) / 32.0).floor() as u32 * 4;
        let pixel_array_size = row_size * height as u32;

        BmpDibHeader {
            header_size: 40,
            width: width,
            height: height,
            num_planes: 1,
            bits_per_pixel: 24,
            compress_type: 0,
            data_size: pixel_array_size,
            hres: 1000,
            vres: 1000,
            num_colors: 0,
            num_imp_colors: 0
        }
    }
}

/// The image type provided by the library.
///
/// It exposes functions to initialize or read BMP images from disk, common modification of pixel
/// data, and saving to disk.
///
/// The image is accessed in row-major order from top to bottom,
/// where point (0, 0) is defined to be in the upper left corner of the image.
///
/// Currently, only uncompressed BMP images are supported.
#[derive(Clone, Eq, PartialEq)]
pub struct Image {
    header: BmpHeader,
    dib_header: BmpDibHeader,
    color_palette: Option<Vec<Pixel>>,
    width: u32,
    height: u32,
    padding: u32,
    data: Vec<Pixel>
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Image {}\n", '{')?;
        write!(f, "\theader: {:?},\n", self.header)?;
        write!(f, "\tdib_header: {:?},\n", self.dib_header)?;
        write!(f, "\tcolor_palette: {:?},\n", self.color_palette)?;
        write!(f, "\twidth: {:?},\n", self.width)?;
        write!(f, "\theight: {:?},\n", self.height)?;
        write!(f, "\tpadding: {:?},\n", self.padding)?;
        write!(f, "{}", '}')
    }
}

impl Image {
    /// Returns a new BMP Image with the `width` and `height` specified. It is initialized to
    /// a black image by default.
    ///
    /// # Example
    ///
    /// ```
    /// extern crate bmp;
    ///
    /// let mut img = bmp::Image::new(100, 80);
    /// ```
    pub fn new(width: u32, height: u32) -> Image {
        let mut data = Vec::with_capacity((width * height) as usize);
        for _ in 0 .. width * height {
            data.push(px!(0, 0, 0));
        }

        let (header_size, data_size) = file_size!(24, width, height);
        Image {
            header: BmpHeader::new(header_size, data_size),
            dib_header: BmpDibHeader::new(width as i32, height as i32),
            color_palette: None,
            width: width,
            height: height,
            padding: width % 4,
            data: data
        }
    }

    /// Returns the `width` of the Image.
    #[inline]
    pub fn get_width(&self) -> u32 {
        self.width
    }

    /// Returns the `height` of the Image.
    #[inline]
    pub fn get_height(&self) -> u32 {
        self.height
    }

    /// Set the pixel value at the position of `width` and `height`.
    ///
    /// # Example
    ///
    /// ```
    /// extern crate bmp;
    ///
    /// let mut img = bmp::Image::new(100, 80);
    /// img.set_pixel(10, 10, bmp::consts::RED);
    /// ```
    #[inline]
    pub fn set_pixel(&mut self, x: u32, y: u32, val: Pixel) {
        self.data[((self.height - y - 1) * self.width + x) as usize] = val;
    }

    /// Returns the pixel value at the position of `width` and `height`.
    ///
    /// # Example
    ///
    /// ```
    /// extern crate bmp;
    ///
    /// let img = bmp::Image::new(100, 80);
    /// assert_eq!(bmp::consts::BLACK, img.get_pixel(10, 10));
    /// ```
    #[inline]
    pub fn get_pixel(&self, x: u32, y: u32) -> Pixel {
        self.data[((self.height - y - 1) * self.width + x) as usize]
    }

    /// Returns a new `ImageIndex` that iterates over the image dimensions in top-bottom order.
    ///
    /// # Example
    ///
    /// ```
    /// extern crate bmp;
    ///
    /// let mut img = bmp::Image::new(100, 100);
    /// for (x, y) in img.coordinates() {
    ///     img.set_pixel(x, y, bmp::consts::BLUE);
    /// }
    /// ```
    #[inline]
    pub fn coordinates(&self) -> ImageIndex {
        ImageIndex::new(self.width as u32, self.height as u32)
    }

    /// Saves the image to the path specified by `name`. The function will overwrite the contents
    /// if a file already exists with the same name.
    ///
    /// The function returns the `io::Result` from the underlying `Reader`.
    ///
    /// # Example
    ///
    /// ```
    /// extern crate bmp;
    ///
    /// let mut img = bmp::Image::new(100, 100);
    /// let _ = img.save("black.bmp").unwrap_or_else(|e| {
    ///     panic!("Failed to save: {}", e)
    /// });
    /// ```
    pub fn save(&self, name: &str) -> io::Result<()> {
        // only 24 bpp encoding supported
        let bmp_data = encoder::encode_image(self)?;
        let mut bmp_file = fs::File::create(name)?;
        bmp_file.write(&bmp_data)?;
        Ok(())
    }
}

/// Returns a `BmpResult`, either containing an `Image` or a `BmpError`.
///
/// # Example
///
/// ```
/// extern crate bmp;
///
/// let img = bmp::open("test/rgbw.bmp").unwrap_or_else(|e| {
///    panic!("Failed to open: {}", e);
/// });
/// ```
pub fn open(name: &str) -> BmpResult<Image> {
    let mut bytes = Vec::new();
    let mut f = fs::File::open(name)?;
    f.read_to_end(&mut bytes)?;
    let mut bmp_data = Cursor::new(bytes);

    decoder::decode_image(&mut bmp_data)
}

/// An `Iterator` returning the `x` and `y` coordinates of an image.
///
/// It supports iteration over an image in row-major order,
/// starting from in the upper left corner of the image.
#[derive(Clone, Copy)]
pub struct ImageIndex {
    width: u32,
    height: u32,
    x: u32,
    y: u32
}

impl ImageIndex {
    fn new(width: u32, height: u32) -> ImageIndex {
        ImageIndex {
            width: width,
            height: height,
            x: 0,
            y: 0
        }
    }
}

impl Iterator for ImageIndex {
    type Item = (u32, u32);

    fn next(&mut self) -> Option<(u32, u32)> {
        if self.x < self.width && self.y < self.height {
            let this = Some((self.x, self.y));
            self.x += 1;
            if self.x == self.width {
                self.x = 0;
                self.y += 1;
            }
            this
        } else {
            None
        }
    }
}
