#![crate_type = "lib"]
#![allow(warnings)]
#![feature(collections)]
#![feature(core, old_io, old_path)]
#![cfg_attr(test, feature(test))]

//! A small library for reading and writing 24-bit BMP images.
//!
//!# Example
//!
//!```
//!extern crate bmp;
//!
//!use bmp::{Image, Pixel};
//!
//!fn main() {
//!    let mut img = Image::new(256, 256);
//!
//!    for (x, y) in img.coordinates() {
//!        img.set_pixel(x, y, Pixel {
//!            r: (x - y / 256) as u8,
//!            g: (y - x / 256) as u8,
//!            b: (x + y / 256) as u8
//!        })
//!    }
//!    let _ = img.save("img.bmp");
//!}
//!
//!```

use std::collections::BitVec;
use std::fmt;
use std::num::Float;
use std::iter::Iterator;
use std::old_io::{File, IoResult, IoError, MemReader, Open, Read, SeekSet, SeekCur};
use std::old_path::Path;
use std::error::{Error, FromError};

const B: u8 = 66;
const M: u8 = 77;

/// The pixel data used in the `Image`
///
/// It has three values for the `red`, `blue` and `green` color channels, respectively.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8
}

/// Common color constants accessible by names.
pub mod consts;

/// A result type, either containing an `Image` or a `BmpError`.
pub type BmpResult<T> = Result<T, BmpError>;

/// The error type returned if the decoding of an image from disk fails.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BmpError {
    WrongMagicNumbers(String),
    UnsupportedBitsPerPixel(String),
    UnsupportedCompressionType,
    UnsupportedBmpVersion(BmpVersion),
    IncorrectDataSize(String),
    IoError(std::old_io::IoError)
}

impl fmt::Display for BmpError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BmpError::WrongMagicNumbers(ref details) =>
                write!(fmt, "Wrong magic numbers: {}", details),
            BmpError::UnsupportedBitsPerPixel(ref details) =>
                write!(fmt, "Unsupported bits per pixel: {}", details),
            BmpError::UnsupportedCompressionType =>
                write!(fmt, "Unsupported compression type: RLE compression not supported"),
            BmpError::UnsupportedBmpVersion(ref version) =>
                write!(fmt, "Unsupported BMP version: {}", version),
            BmpError::IncorrectDataSize(ref details) =>
                write!(fmt, "Incorrect size of image data: {}", details),
            BmpError::IoError(ref error) => error.fmt(fmt)
        }
    }
}

impl FromError<IoError> for BmpError {
    fn from_error(err: IoError) -> BmpError {
        BmpError::IoError(err)
    }
}

impl Error for BmpError {
    fn description(&self) -> &str { "BMP image error" }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BmpVersion {
    Version1,
    Version2,
    Version3,
    Version3NT,
    Version4,
}

impl fmt::Display for BmpVersion {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BmpVersion::Version1 => write!(fmt, "BMP Version 1"),
            BmpVersion::Version2 => write!(fmt, "BMP Version 2"),
            BmpVersion::Version3 => write!(fmt, "BMP Version 3"),
            BmpVersion::Version3NT => write!(fmt, "BMP Version 3 NT"),
            BmpVersion::Version4 => write!(fmt, "BMP Version 4")
        }
    }
}

enum CompressionType {
    Uncompressed,
    RLE_8bit,
    RLE_4bit,
    // Only for BMP version 4
    BitfieldsEncoding,
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
/// Currently, only 24-bit, uncompressed BMP images are supported.
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
        try!(write!(f, "\tdata: {:?},\n", self.data));
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
            data.push(Pixel {r: 0, g: 0, b: 0});
        }

        let padding = width % 4;
        let header_size = 14 + 40;
        let data_size = width * height * 3 + height * padding;
        Image {
            magic: BmpId::new(),
            header: BmpHeader::new(header_size, data_size),
            dib_header: BmpDibHeader::new(width as i32, height as i32),
            color_palette: None,
            width: width,
            height: height,
            padding: padding,
            data: data
        }
    }

    /// Returns the `width` of the Image
    #[inline]
    pub fn get_width(&self) -> u32 {
        self.width
    }

    /// Returns the `height` of the Image
    #[inline]
    pub fn get_height(&self) -> u32 {
        self.height
    }

    /// Set the pixel value at the position of `width` and `height`
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

    /// Returns the pixel value at the position of `width` and `height`
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

    /// Returns a `BmpResult`, either containing an `Image` or a `BmpError`.
    ///
    /// # Example
    ///
    /// ```
    /// extern crate bmp;
    ///
    /// let img = match bmp::Image::open("test/rgbw.bmp") {
    ///     Ok(img) => img,
    ///     Err(e) => panic!("Failed to open: {}", e)
    /// };
    ///
    /// ```
    pub fn open(name: &str) -> BmpResult<Image> {
        let mut f = try!(File::open_mode(&Path::new(name), Open, Read));
        let mut bmp_data = MemReader::new(try!(f.read_to_end()));

        let id = try!(Image::read_bmp_id(&mut bmp_data));
        let header = try!(Image::read_bmp_header(&mut bmp_data));
        let dib_header = try!(Image::read_bmp_dib_header(&mut bmp_data));

        let color_palette = try!(Image::read_color_palette(&mut bmp_data, &dib_header));

        let padding = dib_header.width % 4;
        let data = match color_palette {
            Some(ref palette) => try!(
                Image::read_pixel_indexes(&mut bmp_data, &dib_header,
                                               header.pixel_offset, palette, padding as i64)
            ),
            None => try!(
                Image::read_image_data(&mut bmp_data, &dib_header,
                                            header.pixel_offset, padding as i64)
            )
        };

        let width = dib_header.width;
        let height = dib_header.height;

        let image = Image {
            magic: id,
            header: header,
            dib_header: dib_header,
            color_palette: color_palette,
            width: width as u32,
            height: height as u32,
            padding: padding as u32,
            data: data
        };

        Ok(image)
    }

    /// Saves the image to the path specified by `name`. The function will overwrite the contents
    /// if a file already exists with the same name.
    ///
    /// The function returns the `IoResult` returned from the underlying `Reader`.
    ///
    /// # Example
    ///
    /// ```
    /// extern crate bmp;
    ///
    /// let mut img = bmp::Image::new(100, 100);
    /// match img.save("black.bmp") {
    ///     Ok(_) => (/* Success */),
    ///     Err(e) => panic!("Failed to save: {}", e)
    /// }
    /// ```
    pub fn save(&self, name: &str) -> IoResult<()> {
        // only 24 bpp encoding supported
        let mut bmp_data = Vec::with_capacity(self.header.file_size as usize);
        try!(self.write_header(&mut bmp_data));
        try!(self.write_data(&mut bmp_data));

        let mut bmp_file = try!(File::create(&Path::new(name)));
        try!(bmp_file.write_all(&bmp_data[..]));
        Ok(())
    }

    fn write_header(&self, bmp_data: &mut Vec<u8>) -> IoResult<()> {
        let id = &self.magic;
        try!(bmp_data.write_all(&[id.magic1, id.magic2]));

        let header = &self.header;
        let dib_header = &self.dib_header;
        let (header_size, data_size) = file_size!(24, dib_header.width, dib_header.height);

        try!(bmp_data.write_le_u32(header_size + data_size));
        try!(bmp_data.write_le_u16(header.creator1));
        try!(bmp_data.write_le_u16(header.creator2));
        try!(bmp_data.write_le_u32(header_size)); // pixel_offset

        try!(bmp_data.write_le_u32(dib_header.header_size));
        try!(bmp_data.write_le_i32(dib_header.width));
        try!(bmp_data.write_le_i32(dib_header.height));
        try!(bmp_data.write_le_u16(dib_header.num_planes));
        try!(bmp_data.write_le_u16(24)); // bits_per_pixel
        try!(bmp_data.write_le_u32(dib_header.compress_type));
        try!(bmp_data.write_le_u32(dib_header.data_size));
        try!(bmp_data.write_le_i32(dib_header.hres));
        try!(bmp_data.write_le_i32(dib_header.vres));
        try!(bmp_data.write_le_u32(0)); // num_colors
        try!(bmp_data.write_le_u32(dib_header.num_imp_colors));
        Ok(())
    }

    fn write_data(&self, bmp_data: &mut Vec<u8>) -> IoResult<()> {
        let padding: &[u8] = &[0; 4][0 .. self.padding as usize];
        for y in (0 .. self.height) {
            for x in (0 .. self.width) {
                let index = (y * self.width + x) as usize;
                let px = &self.data[index];
                try!(bmp_data.write_all(&[px.b, px.g, px.r]));
            }
            try!(bmp_data.write_all(padding));
        }
        Ok(())
    }

    fn read_bmp_id(bmp_data: &mut MemReader) -> BmpResult<BmpId> {
        let (m1, m2) = (try!(bmp_data.read_byte()), try!(bmp_data.read_byte()));

        match (m1, m2) {
            (B, M) => Ok(BmpId::new()),
            (m1, m2) => Err(BmpError::WrongMagicNumbers(
                            format!("Expected '66 77', but was '{} {}'", m1, m2))),
        }
    }

    fn read_bmp_header(bmp_data: &mut MemReader) -> BmpResult<BmpHeader> {
        let header = BmpHeader {
            file_size:    try!(bmp_data.read_le_u32()),
            creator1:     try!(bmp_data.read_le_u16()),
            creator2:     try!(bmp_data.read_le_u16()),
            pixel_offset: try!(bmp_data.read_le_u32())
        };

        Ok(header)
    }

    fn read_bmp_dib_header(bmp_data: &mut MemReader) -> BmpResult<BmpDibHeader> {
        let dib_header = BmpDibHeader {
            header_size:    try!(bmp_data.read_le_u32()),
            width:          try!(bmp_data.read_le_i32()),
            height:         try!(bmp_data.read_le_i32()),
            num_planes:     try!(bmp_data.read_le_u16()),
            bits_per_pixel: try!(bmp_data.read_le_u16()),
            compress_type:  try!(bmp_data.read_le_u32()),
            data_size:      try!(bmp_data.read_le_u32()),
            hres:           try!(bmp_data.read_le_i32()),
            vres:           try!(bmp_data.read_le_i32()),
            num_colors:     try!(bmp_data.read_le_u32()),
            num_imp_colors: try!(bmp_data.read_le_u32()),
        };

        match dib_header.header_size {
            // BMPv2 has a header size of 12 bytes
            12 => return Err(BmpError::UnsupportedBmpVersion(BmpVersion::Version2)),
            // BMPv3 has a header size of 40 bytes, it is NT if the compression type is 3
            40 if dib_header.compress_type == 3 =>
                return Err(BmpError::UnsupportedBmpVersion(BmpVersion::Version3NT)),
            // BMPv4 has more data in its header, it is currently ignored but we still try to parse it
            108 | _ => ()
        }

        match dib_header.bits_per_pixel {
            // Currently supported
            1 | 24 => (),
            other => return Err(
                BmpError::UnsupportedBitsPerPixel(format!("{}", dib_header.bits_per_pixel))
            )
        }

        if dib_header.compress_type != 0 {
            return Err(BmpError::UnsupportedCompressionType);
        }

        let row_size = ((24.0 * dib_header.width as f32 + 31.0) / 32.0).floor() as u32 * 4;
        let pixel_array_size = row_size * dib_header.height as u32;
        if pixel_array_size != dib_header.data_size {
            return Err(BmpError::IncorrectDataSize(
                format!("Expected {}, but was {}", pixel_array_size, dib_header.data_size)))
        }

        Ok(dib_header)
    }

    fn read_color_palette(bmp_data: &mut MemReader, dh: &BmpDibHeader) ->
                          BmpResult<Option<Vec<Pixel>>> {
        let num_entries = match dh.bits_per_pixel {
            // We have a color_palette if there are 8 or less bits per pixel
            bpp @ 1 | bpp @ 4 | bpp @ 8 => 1 << bpp,
            _ if dh.num_colors == 0 => return Ok(None),
            // Or if num_colors in the dib header is not zero
            _ => dh.num_colors as usize
        };

        let num_bytes = match dh.header_size {
            // Each entry in the color_palette is four bytes for Version 3 or 4
            40 | 108 => 4,
            // Three bytes for Version two. Though, this is currently not supported
            _ => return Err(BmpError::UnsupportedBmpVersion(BmpVersion::Version2))
        };

        let mut px = [0; 3];
        let mut color_palette = Vec::with_capacity(num_entries);
        for _ in 0 .. num_entries {
            try!(bmp_data.read(&mut px));
            color_palette.push(Pixel {r: px[2], g: px[1], b: px[0]});
            if num_bytes == 4 {
                // Ignore the extra byte reserved for padding
                let _ = bmp_data.read_byte();
            }
        }

        Ok(Some(color_palette))
    }

    fn read_pixel_indexes(bmp_data: &mut MemReader, dh: &BmpDibHeader,
                          offset: u32, palette: &Vec<Pixel>, padding: i64) -> BmpResult<Vec<Pixel>> {
        let num_bits = dh.width;
        let num_bytes = Float::ceil(num_bits as f64 / 8.0) as usize;
        let overflow = 8 - num_bits % 8;

        let mut data = Vec::with_capacity((dh.height * dh.width) as usize);
        // seek until data
        try!(bmp_data.seek(offset as i64, SeekSet));
        for _ in 0 .. dh.height {
            let bytes = try!(bmp_data.read_exact(num_bytes));
            let bits = BitVec::from_bytes(&bytes[..]);
            // only iterate over the first
            for b in 0 .. num_bits as usize {
                match bits[b] {
                    true => data.push(palette[1]),
                    false => data.push(palette[0])
                }
            }
            try!(bmp_data.seek(padding, SeekCur));
        }
        Ok(data)
    }

    fn read_image_data(bmp_data: &mut MemReader, dh: &BmpDibHeader, offset: u32, padding: i64) ->
                       BmpResult<Vec<Pixel>> {
        let mut data = Vec::with_capacity((dh.height * dh.width) as usize);
        // seek until data
        try!(bmp_data.seek(offset as i64, SeekSet));
        // read pixels until padding
        let mut px = [0; 3];
        for _ in 0 .. dh.height {
            for _ in 0 .. dh.width {
                try!(bmp_data.read(&mut px));
                data.push(Pixel {r: px[2], g: px[1], b: px[0]});
            }
            // seek padding
            try!(bmp_data.seek(padding, SeekCur));
        }
        Ok(data)
    }
}

/// An `Iterator` returning the `x` and `y` coordinates of an image.
///
/// It supports iteration over an image in row-major order, starting from in the upper left corner of the image.
#[derive(Copy)]
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

#[cfg(test)]
mod tests {
    extern crate test;

    use std::mem::size_of;
    use std::old_io::{File, SeekSet};
    use std::old_io::fs::PathExtensions;

    use {B, M, BmpError, BmpId, BmpHeader, BmpDibHeader, Image, Pixel};
    use consts;
    use consts::{RED, LIME, BLUE, WHITE};

    #[test]
    fn size_of_bmp_header_is_54_bytes() {
        let bmp_magic_size = size_of::<BmpId>();
        let bmp_header_size = size_of::<BmpHeader>();
        let bmp_bip_header_size = size_of::<BmpDibHeader>();

        assert_eq!(2,  bmp_magic_size);
        assert_eq!(12, bmp_header_size);
        assert_eq!(40, bmp_bip_header_size);
    }

    #[test]
    fn size_of_4pixel_bmp_image_is_70_bytes() {
        let path_wd = Path::new("test/rgbw.bmp");
        match path_wd.lstat() {
            Ok(stat) => assert_eq!(70, stat.size as i32),
            Err(_) => (/* Ignore IoError for now */)
        }
    }

    fn verify_test_bmp_image(img: Image) {
        let header = img.header;
        assert_eq!(70, header.file_size);
        assert_eq!(0,  header.creator1);
        assert_eq!(0,  header.creator2);

        let dib_header = img.dib_header;
        assert_eq!(54, header.pixel_offset);
        assert_eq!(40,    dib_header.header_size);
        assert_eq!(2,     dib_header.width);
        assert_eq!(2,     dib_header.height);
        assert_eq!(1,     dib_header.num_planes);
        assert_eq!(24,    dib_header.bits_per_pixel);
        assert_eq!(0,     dib_header.compress_type);
        assert_eq!(16,    dib_header.data_size);
        assert_eq!(1000, dib_header.hres);
        assert_eq!(1000, dib_header.vres);
        assert_eq!(0,     dib_header.num_colors);
        assert_eq!(0,     dib_header.num_imp_colors);

        assert_eq!(2, img.padding);
    }

    #[test]
    fn can_read_bmp_image() {
        let bmp_img = Image::open("test/rgbw.bmp").unwrap();
        verify_test_bmp_image(bmp_img);
    }

    #[test]
    fn can_read_image_data() {
        let mut f = match File::open(&Path::new("test/rgbw.bmp")) {
            Ok(file) => file,
            Err(e) => panic!("File error: {}", e)
        };
        assert_eq!(B, f.read_byte().unwrap());
        assert_eq!(M, f.read_byte().unwrap());

        match f.seek(54, SeekSet) {
            Ok(_) => (),
            Err(e) => panic!("Seek error: {}", e)
        }

        let pixel = Pixel {
            r: f.read_byte().unwrap(),
            g: f.read_byte().unwrap(),
            b: f.read_byte().unwrap()
        };

        assert_eq!(pixel, RED);
    }

    #[test]
    fn can_read_entire_bmp_image() {
        let bmp_img = Image::open("test/rgbw.bmp").unwrap();
        assert_eq!(bmp_img.data.len(), 4);

        assert_eq!(bmp_img.get_pixel(0, 0), RED);
        assert_eq!(bmp_img.get_pixel(1, 0), LIME);
        assert_eq!(bmp_img.get_pixel(0, 1), BLUE);
        assert_eq!(bmp_img.get_pixel(1, 1), WHITE);
    }

    #[test]
    fn read_write_1pbb_bmp_image() {
        let img = Image::open("test/bmptestsuite-0.9/valid/1bpp-1x1.bmp").unwrap();
        assert_eq!(img.data.len(), 1);
        assert_eq!(img.get_pixel(0, 0), consts::BLACK);

        let _ = img.save("test/1bb-1x1.bmp");
        let img = Image::open("test/1bb-1x1.bmp").unwrap();
        assert_eq!(img.data.len(), 1);
        assert_eq!(img.get_pixel(0, 0), consts::BLACK);
    }

    #[test]
    fn error_when_opening_unexisting_image() {
        let result = Image::open("test/no_img.bmp");
        match result {
            Err(BmpError::IoError(_)) => (/* Expected */),
            _ => panic!("Ghost image!?")
        }
    }

    #[test]
    fn error_when_opening_image_with_wrong_bits_per_pixel() {
        let result = Image::open("test/bmptestsuite-0.9/valid/4bpp-1x1.bmp");
        match result {
            Err(BmpError::UnsupportedBitsPerPixel(_)) => (/* Expected */),
            _ => panic!("4bpp should not be supported")
        }
    }

    #[test]
    fn error_when_opening_image_with_wrong_magic_numbers() {
        let result = Image::open("test/bmptestsuite-0.9/corrupt/magicnumber-bad.bmp");
        match result {
            Err(BmpError::WrongMagicNumbers(_)) => (/* Expected */),
            _ => panic!("Wrong magic numbers should not be supported")
        }
    }

    #[test]
    fn can_create_bmp_file() {
        let mut bmp = Image::new(2, 2);
        bmp.set_pixel(0, 0, RED);
        bmp.set_pixel(1, 0, LIME);
        bmp.set_pixel(0, 1, BLUE);
        bmp.set_pixel(1, 1, WHITE);
        let _ = bmp.save("test/rgbw_test.bmp");

        let bmp_img = Image::open("test/rgbw_test.bmp").unwrap();
        assert_eq!(bmp_img.get_pixel(0, 0), RED);
        assert_eq!(bmp_img.get_pixel(1, 0), LIME);
        assert_eq!(bmp_img.get_pixel(0, 1), BLUE);
        assert_eq!(bmp_img.get_pixel(1, 1), WHITE);

        verify_test_bmp_image(bmp_img);
    }

    #[test]
    fn changing_pixels_does_not_push_image_data() {
        let mut img = Image::new(2, 1);
        img.set_pixel(1, 0, WHITE);
        img.set_pixel(0, 0, WHITE);

        assert_eq!(img.get_pixel(0, 0), WHITE);
        assert_eq!(img.get_pixel(1, 0), WHITE);
    }

    #[test]
    fn coordinates_iterator_gives_x_and_y_in_row_major_order() {
        let img = Image::new(2, 3);
        let mut coords = img.coordinates();
        assert_eq!(coords.next(), Some((0, 0)));
        assert_eq!(coords.next(), Some((1, 0)));
        assert_eq!(coords.next(), Some((0, 1)));
        assert_eq!(coords.next(), Some((1, 1)));
        assert_eq!(coords.next(), Some((0, 2)));
        assert_eq!(coords.next(), Some((1, 2)));
    }

    #[bench]
    fn write_bmp(b: &mut test::Bencher) {
        let img = Image::new(320, 240);
        b.iter(|| img.save("test/bench_test.bmp"));
    }

    #[bench]
    fn open_bmp(b: &mut test::Bencher) {
        b.iter(|| Image::open("test/bmptestsuite-0.9/valid/24bpp-320x240.bmp"));
    }
}
