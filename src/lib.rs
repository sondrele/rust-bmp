#![crate_type = "lib"]
#![warn(warnings)]
#![feature(collections)]
#![feature(convert)]
#![feature(core)]
#![feature(io)]
#![cfg_attr(test, feature(test))]
#![cfg_attr(test, feature(path_ext))]

//! A small library for reading and writing BMP images.
//!
//! The library supports uncompressed BMP Version 3 images.
//! The different decoding and encoding schemes is shown in the table below.
//!
//! |Scheme | Decoding | Encoding | Compression |
//! |-------|----------|----------|-------------|
//! | 24 bpp| ✓        | ✓       | No
//! | 8 bpp | ✓        | ✗       | No
//! | 4 bpp | ✓        | ✗       | No
//! | 1 bpp | ✓        | ✗       | No
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
//!         img.set_pixel(x, y, px!(x, y, x));
//!     }
//!     let _ = img.save("img.bmp");
//! }
//! ```
//!

extern crate byteorder;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use std::collections::BitVec;
use std::convert::{From, AsRef};
use std::error::Error;
use std::fmt;
use std::fs;
use std::num::Float;
use std::num::SignedInt;
use std::io;
use std::io::{Cursor, Read, Write, SeekFrom, Seek};
use std::iter::Iterator;

use ::CompressionType::*;
use ::BmpErrorKind::*;
use ::BmpVersion::*;

#[cfg(test)]
mod tests;

const B: u8 = 66;
const M: u8 = 77;

/// The pixel data used in the `Image`.
///
/// It has three values for the `red`, `blue` and `green` color channels, respectively.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8
}

/// Macro to generate a `Pixel` from `r`, `g` and `b` values.
#[macro_export]
macro_rules! px {
    ($r:expr, $g:expr, $b:expr) => {
        Pixel { r: $r as u8, g: $g as u8, b: $b as u8 }
    }
}

/// Common color constants accessible by names.
pub mod consts;

/// A result type, either containing an `Image` or a `BmpError`.
pub type BmpResult<T> = Result<T, BmpError>;

/// The different kinds of possible BMP errors.
#[derive(Debug)]
pub enum BmpErrorKind {
    WrongMagicNumbers,
    UnsupportedBitsPerPixel,
    UnsupportedCompressionType,
    UnsupportedBmpVersion,
    Other,
    BmpIoError(io::Error),
    BmpByteorderError(byteorder::Error),
}

/// The error type returned if the decoding of an image from disk fails.
#[derive(Debug)]
pub struct BmpError {
    pub kind: BmpErrorKind,
    pub details: String,
}

impl BmpError {
    fn new<T: AsRef<str>>(kind: BmpErrorKind, details: T) -> BmpError {
        let description = match kind {
            WrongMagicNumbers => "Wrong magic numbers",
            UnsupportedBitsPerPixel => "Unsupported bits per pixel",
            UnsupportedCompressionType => "Unsupported compression type",
            UnsupportedBmpVersion => "Unsupported BMP version",
            _ => "BMP Error",
        };

        BmpError {
            kind: kind,
            details: format!("{}: {}", description, details.as_ref())
        }
    }
}

impl fmt::Display for BmpError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            BmpIoError(ref error) => return error.fmt(fmt),
            _ => write!(fmt, "{}", self.description())
        }
    }
}

impl From<io::Error> for BmpError {
    fn from(err: io::Error) -> BmpError {
        BmpError::new(BmpIoError(err), "Io Error")
    }
}

impl From<byteorder::Error> for BmpError {
    fn from(err: byteorder::Error) -> BmpError {
        BmpError::new(BmpByteorderError(err), "Byteorder Error")
    }
}

impl Error for BmpError {
    fn description(&self) -> &str {
        match self.kind {
            BmpIoError(ref e) => Error::description(e),
            BmpByteorderError(ref e) => Error::description(e),
            _ => &self.details
        }
    }
}

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
struct BmpId {
    magic1: u8,
    magic2: u8
}

impl BmpId {
    pub fn new() -> BmpId {
        BmpId {
            magic1: B,
            magic2: M
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
    magic: BmpId,
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
        try!(write!(f, "Image {}\n", '{'));
        try!(write!(f, "\tmagic: {:?},\n", self.magic));
        try!(write!(f, "\theader: {:?},\n", self.header));
        try!(write!(f, "\tdib_header: {:?},\n", self.dib_header));
        try!(write!(f, "\tcolor_palette: {:?},\n", self.color_palette));
        try!(write!(f, "\twidth: {:?},\n", self.width));
        try!(write!(f, "\theight: {:?},\n", self.height));
        try!(write!(f, "\tpadding: {:?},\n", self.padding));
        write!(f, "{}", '}')
    }
}

macro_rules! file_size {
    ($bpp:expr, $width:expr, $height:expr) => {{
        let header_size = 2 + 12 + 40;
        let row_size = (($bpp as f32 * $width as f32 + 31.0) / 32.0).floor() as u32 * 4;
        (header_size as u32, $height as u32 * row_size)
    }}
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
            magic: BmpId::new(),
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
        let mut bmp_data = Vec::with_capacity(self.header.file_size as usize);
        try!(self.write_header(&mut bmp_data));
        try!(self.write_data(&mut bmp_data));

        let mut bmp_file = try!(fs::File::create(name));
        try!(bmp_file.write(&bmp_data));
        Ok(())
    }

    fn write_header(&self, bmp_data: &mut Vec<u8>) -> io::Result<()> {
        let header = &self.header;
        let dib_header = &self.dib_header;
        let (header_size, data_size) = file_size!(24, self.width, self.height);

        try!(std::io::Write::write(bmp_data, &[B, M]));

        try!(bmp_data.write_u32::<LittleEndian>(header_size + data_size));
        try!(bmp_data.write_u16::<LittleEndian>(header.creator1));
        try!(bmp_data.write_u16::<LittleEndian>(header.creator2));
        try!(bmp_data.write_u32::<LittleEndian>(header_size)); // pixel_offset

        try!(bmp_data.write_u32::<LittleEndian>(dib_header.header_size));
        try!(bmp_data.write_i32::<LittleEndian>(dib_header.width));
        try!(bmp_data.write_i32::<LittleEndian>(dib_header.height));
        try!(bmp_data.write_u16::<LittleEndian>(1));  // num_planes
        try!(bmp_data.write_u16::<LittleEndian>(24)); // bits_per_pixel
        try!(bmp_data.write_u32::<LittleEndian>(0));  // compress_type
        try!(bmp_data.write_u32::<LittleEndian>(data_size));
        try!(bmp_data.write_i32::<LittleEndian>(dib_header.hres));
        try!(bmp_data.write_i32::<LittleEndian>(dib_header.vres));
        try!(bmp_data.write_u32::<LittleEndian>(0)); // num_colors
        try!(bmp_data.write_u32::<LittleEndian>(0)); // num_imp_colors
        Ok(())
    }

    fn write_data(&self, bmp_data: &mut Vec<u8>) -> io::Result<()> {
        let padding = &[0; 4][0 .. self.padding as usize];
        for y in 0 .. self.height {
            for x in 0 .. self.width {
                let index = (y * self.width + x) as usize;
                let px = &self.data[index];
                try!(Write::write(bmp_data, &[px.b, px.g, px.r]));
            }
            try!(Write::write(bmp_data, padding));
        }
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
    let mut f = try!(fs::File::open(name));
    try!(f.read_to_end(&mut bytes));
    let mut bmp_data = Cursor::new(bytes);

    let id = try!(read_bmp_id(&mut bmp_data));
    let header = try!(read_bmp_header(&mut bmp_data));
    let dib_header = try!(read_bmp_dib_header(&mut bmp_data));

    let color_palette = try!(read_color_palette(&mut bmp_data, &dib_header));

    let width = dib_header.width.abs() as u32;
    let height = dib_header.height.abs() as u32;
    let padding = width % 4;

    let data = match color_palette {
        Some(ref palette) => try!(
            read_indexes(&mut bmp_data.into_inner(), &palette, width as usize, height as usize,
                         dib_header.bits_per_pixel, header.pixel_offset as usize)
        ),
        None => try!(
            read_pixels(&mut bmp_data, width, height, header.pixel_offset, padding as i64)
        )
    };

    let image = Image {
        magic: id,
        header: header,
        dib_header: dib_header,
        color_palette: color_palette,
        width: width,
        height: height,
        padding: padding,
        data: data
    };

    Ok(image)
}

fn read_bmp_id(bmp_data: &mut Cursor<Vec<u8>>) -> BmpResult<BmpId> {
    let mut bm = [0, 0];
    try!(bmp_data.read(&mut bm));

    if bm == b"BM"[..] {
        Ok(BmpId::new())
    } else {
        Err(BmpError::new(WrongMagicNumbers, format!("Expected [66, 77], but was {:?}", bm)))
    }
}

fn read_bmp_header(bmp_data: &mut Cursor<Vec<u8>>) -> BmpResult<BmpHeader> {
    let header = BmpHeader {
        file_size:    try!(bmp_data.read_u32::<LittleEndian>()),
        creator1:     try!(bmp_data.read_u16::<LittleEndian>()),
        creator2:     try!(bmp_data.read_u16::<LittleEndian>()),
        pixel_offset: try!(bmp_data.read_u32::<LittleEndian>()),
    };

    Ok(header)
}

fn read_bmp_dib_header(bmp_data: &mut Cursor<Vec<u8>>) -> BmpResult<BmpDibHeader> {
    let dib_header = BmpDibHeader {
        header_size:    try!(bmp_data.read_u32::<LittleEndian>()),
        width:          try!(bmp_data.read_i32::<LittleEndian>()),
        height:         try!(bmp_data.read_i32::<LittleEndian>()),
        num_planes:     try!(bmp_data.read_u16::<LittleEndian>()),
        bits_per_pixel: try!(bmp_data.read_u16::<LittleEndian>()),
        compress_type:  try!(bmp_data.read_u32::<LittleEndian>()),
        data_size:      try!(bmp_data.read_u32::<LittleEndian>()),
        hres:           try!(bmp_data.read_i32::<LittleEndian>()),
        vres:           try!(bmp_data.read_i32::<LittleEndian>()),
        num_colors:     try!(bmp_data.read_u32::<LittleEndian>()),
        num_imp_colors: try!(bmp_data.read_u32::<LittleEndian>()),
    };

    match dib_header.header_size {
        // BMPv2 has a header size of 12 bytes
        12 => return Err(BmpError::new(UnsupportedBmpVersion, Version2)),
        // BMPv3 has a header size of 40 bytes, it is NT if the compression type is 3
        40 if dib_header.compress_type == 3 =>
            return Err(BmpError::new(UnsupportedBmpVersion, Version3NT)),
        // BMPv4 has more data in its header, it is currently ignored but we still try to parse it
        108 | _ => ()
    }

    match dib_header.bits_per_pixel {
        // Currently supported
        1 | 4 | 8 | 24 => (),
        other => return Err(
            BmpError::new(UnsupportedBitsPerPixel, format!("{}", other))
        )
    }

    match CompressionType::from_u32(dib_header.compress_type) {
        Uncompressed => (),
        other => return Err(BmpError::new(UnsupportedCompressionType, other)),
    }

    Ok(dib_header)
}

fn read_color_palette(bmp_data: &mut Cursor<Vec<u8>>, dh: &BmpDibHeader) ->
                      BmpResult<Option<Vec<Pixel>>> {
    let num_entries = match dh.bits_per_pixel {
        // We have a color_palette if there if num_colors in the dib header is not zero
        _ if dh.num_colors != 0 => dh.num_colors as usize,
        // Or if there are 8 or less bits per pixel
        bpp @ 1 | bpp @ 4 | bpp @ 8 => 1 << bpp,
        _ => return Ok(None)
    };

    let num_bytes = match dh.header_size {
        // Each entry in the color_palette is four bytes for Version 3 or 4
        40 | 108 => 4,
        // Three bytes for Version two. Though, this is currently not supported
        _ => return Err(BmpError::new(UnsupportedBmpVersion, Version2))
    };

    let mut px = &mut [0; 4][0 .. num_bytes as usize];
    let mut color_palette = Vec::with_capacity(num_entries);
    for _ in 0 .. num_entries {
        try!(bmp_data.read(&mut px));
        color_palette.push(px!(px[2], px[1], px[0]));
    }

    Ok(Some(color_palette))
}

fn read_indexes(bmp_data: &mut Vec<u8>, palette: &Vec<Pixel>,
                width: usize, height: usize, bpp: u16, offset: usize) -> BmpResult<Vec<Pixel>> {
    let mut data = Vec::with_capacity(height * width);
    // Number of bytes to read from each row, varies based on bits_per_pixel
    let bytes_per_row = Float::ceil(width as f64 / (8.0 / bpp as f64)) as usize;
    for y in 0 .. height {
        let padding = match bytes_per_row % 4 {
            0 => 0,
            other => 4 - other
        };
        let start = offset + (bytes_per_row + padding) * y;
        let bytes = &bmp_data[start .. start + bytes_per_row];

        // determine how to parse each row, depending on bits_per_pixel
        match bpp {
            1 => {
                let bits = BitVec::from_bytes(&bytes[..]);
                for b in 0 .. width as usize {
                    match bits[b] {
                        true => data.push(palette[1]),
                        false => data.push(palette[0])
                    }
                }
            },
            4 => {
                let mut index = Vec::with_capacity(data.len() + 1);
                for b in bytes {
                    index.push((b >> 4));
                    index.push((b & 0x0f));
                }
                for i in 0 .. width as usize {
                    data.push(palette[index[i] as usize]);
                }
            },
            8 => {
                for index in bytes {
                    data.push(palette[*index as usize]);
                }
            },
            other => return Err(BmpError::new(Other,
                format!("BMP does not support color palettes for {} bits per pixel", other)))
        }
    }
    Ok(data)
}

fn read_pixels(bmp_data: &mut Cursor<Vec<u8>>, width: u32, height: u32,
               offset: u32, padding: i64) -> BmpResult<Vec<Pixel>> {
    let mut data = Vec::with_capacity((height * width) as usize);
    // seek until data
    try!(bmp_data.seek(SeekFrom::Start(offset as u64)));
    // read pixels until padding
    let mut px = [0; 3];
    for _ in 0 .. height {
        for _ in 0 .. width {
            try!(bmp_data.read(&mut px));
            data.push(px!(px[2], px[1], px[0]));
        }
        // seek padding
        try!(bmp_data.seek(SeekFrom::Current(padding)));
    }
    Ok(data)
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
