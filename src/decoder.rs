extern crate byteorder;

use byteorder::{LittleEndian, ReadBytesExt};

use std::collections::BitVec;
use std::convert::{From, AsRef};
use std::error::Error;
use std::fmt;
use std::io::{self, Cursor, Read, Write, SeekFrom, Seek};

use {BmpId, BmpHeader, BmpDibHeader, CompressionType, Image, Pixel};
use BmpVersion::*;

use self::BmpErrorKind::*;

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

pub fn decode_image(bmp_data: &mut Cursor<Vec<u8>>) -> BmpResult<Image> {
    let id = try!(read_bmp_id(bmp_data));
    let header = try!(read_bmp_header(bmp_data));
    let dib_header = try!(read_bmp_dib_header(bmp_data));

    let color_palette = try!(read_color_palette(bmp_data, &dib_header));

    let width = dib_header.width.abs() as u32;
    let height = dib_header.height.abs() as u32;
    let padding = width % 4;

    let data = match color_palette {
        Some(ref palette) => try!(
            read_indexes(bmp_data.get_mut(), &palette, width as usize, height as usize,
                         dib_header.bits_per_pixel, header.pixel_offset as usize)
        ),
        None => try!(
            read_pixels(bmp_data, width, height, header.pixel_offset, padding as i64)
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
        CompressionType::Uncompressed => (),
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
    let bytes_per_row = (width as f64 / (8.0 / bpp as f64)).ceil() as usize;
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
