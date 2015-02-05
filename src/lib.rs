#![crate_type = "lib"]
#![deny(warnings)]
#![feature(core, io, path)]

use std::fmt;
use std::num::Float;
use std::iter::Iterator;
use std::old_io::{BufferedStream, File, IoResult, IoError, Open, Read, SeekSet, SeekCur};
use std::error::{Error, FromError};

const B: u8 = 66;
const M: u8 = 77;

#[derive(Debug, PartialEq, Copy)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8
}

pub mod consts;

pub type BmpResult<T> = Result<T, BmpError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BmpError {
    WrongMagicNumbers(String),
    UnsupportedBitsPerPixel(String),
    IncorrectDataSize(String),
    IoError(std::old_io::IoError)
}

impl fmt::Display for BmpError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BmpError::WrongMagicNumbers(ref detail) =>
                write!(fmt, "Wrong magic numbers: {}", detail),
            BmpError::UnsupportedBitsPerPixel(ref detail) =>
                write!(fmt, "Unsupported bits per pixel: {}", detail),
            BmpError::IncorrectDataSize(ref detail) =>
                write!(fmt, "Incorrect size of image data: {}", detail),
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

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
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

pub struct Image {
    magic: BmpId,
    header: BmpHeader,
    dib_header: BmpDibHeader,
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
        try!(write!(f, "\twidth: {:?},\n", self.width));
        try!(write!(f, "\theight: {:?},\n", self.height));
        write!(f, "{}", '}')
    }
}

impl Image {
    pub fn new(width: u32, height: u32) -> Image {
        let mut data = Vec::with_capacity((width * height) as usize);
        for _ in (0 .. width * height) {
            data.push(Pixel {r: 0, g: 0, b: 0});
        }

        let padding = width % 4;
        let header_size = 54;
        let data_size = width * height * 3 + height * padding;
        Image {
            magic: BmpId::new(),
            header: BmpHeader::new(header_size, data_size),
            dib_header: BmpDibHeader::new(width as i32, height as i32),
            width: width,
            height: height,
            padding: padding,
            data: data
        }
    }

    pub fn get_width(&self) -> u32 {
        self.width
    }

    pub fn get_height(&self) -> u32 {
        self.height
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, val: Pixel) {
        self.data[((self.height - y - 1) * self.width + x) as usize] = val;
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> Pixel {
        self.data[((self.height - y - 1) * self.width + x) as usize]
    }

    pub fn coordinates(&self) -> ImageIndex {
        ImageIndex::new(self.width as u32, self.height as u32)
    }

    pub fn open(name: &str) -> BmpResult<Image> {
        let mut f = try!(File::open_mode(&Path::new(name), Open, Read));

        let id = try!(Image::read_bmp_id(&mut f));
        let header = try!(Image::read_bmp_header(&mut f));
        let dib_header = try!(Image::read_bmp_dib_header(&mut f));

        let padding = dib_header.width % 4;
        let data = try!(Image::read_image_data(&mut f, &dib_header,
                                                header.pixel_offset,
                                                padding as i64));

        let width = dib_header.width;
        let height = dib_header.height;

        let image = Image {
            magic: id,
            header: header,
            dib_header: dib_header,
            width: width as u32,
            height: height as u32,
            padding: padding as u32,
            data: data
        };

        Ok(image)
    }

    pub fn save(&self, name: &str) -> IoResult<()> {
        let mut f = try!(File::create(&Path::new(name)));

        try!(self.write_header(&mut f));
        try!(self.write_data(f));
        Ok(())
    }

    fn write_header(&self, f: &mut File) -> IoResult<()> {
        let id = &self.magic;
        try!(f.write_all(&[id.magic1, id.magic2]));

        let header = &self.header;
        try!(f.write_le_u32(header.file_size));
        try!(f.write_le_u16(header.creator1));
        try!(f.write_le_u16(header.creator2));
        try!(f.write_le_u32(header.pixel_offset));

        let dib_header = &self.dib_header;
        try!(f.write_le_u32(dib_header.header_size));
        try!(f.write_le_i32(dib_header.width));
        try!(f.write_le_i32(dib_header.height));
        try!(f.write_le_u16(dib_header.num_planes));
        try!(f.write_le_u16(dib_header.bits_per_pixel));
        try!(f.write_le_u32(dib_header.compress_type));
        try!(f.write_le_u32(dib_header.data_size));
        try!(f.write_le_i32(dib_header.hres));
        try!(f.write_le_i32(dib_header.vres));
        try!(f.write_le_u32(dib_header.num_colors));
        try!(f.write_le_u32(dib_header.num_imp_colors));
        Ok(())
    }

    fn write_data(&self, file: File) -> IoResult<()> {
        let mut stream = BufferedStream::new(file);

        let padding: &[u8] = &[0; 4][0 .. self.padding as usize];
        for y in (0 .. self.height) {
            for x in (0 .. self.width) {
                let index = (y * self.width + x) as usize;
                let px = &self.data[index];
                try!(stream.write_all(&[px.b, px.g, px.r]));
            }
            try!(stream.write_all(padding));
        }
        Ok(())
    }

    fn read_bmp_id(f: &mut File) -> BmpResult<BmpId> {
        let (m1, m2) = (try!(f.read_byte()), try!(f.read_byte()));

        match (m1, m2) {
            (m1, m2) if m1 != B || m2 != M =>
                Err(BmpError::WrongMagicNumbers(
                    format!("Expected '66 77', but was '{} {}'", m1, m2))),
            (m1, m2) => Ok(BmpId { magic1: m1, magic2: m2 })
        }
    }

    fn read_bmp_header(f: &mut File) -> BmpResult<BmpHeader> {
        let header = BmpHeader {
            file_size: try!(f.read_le_u32()),
            creator1: try!(f.read_le_u16()),
            creator2: try!(f.read_le_u16()),
            pixel_offset: try!(f.read_le_u32())
        };

        Ok(header)
    }

    fn read_bmp_dib_header(f: &mut File) -> BmpResult<BmpDibHeader> {
        let dib_header = BmpDibHeader {
            header_size: try!(f.read_le_u32()),
            width: try!(f.read_le_i32()),
            height: try!(f.read_le_i32()),
            num_planes: try!(f.read_le_u16()),
            bits_per_pixel: try!(f.read_le_u16()),
            compress_type: try!(f.read_le_u32()),
            data_size: try!(f.read_le_u32()),
            hres: try!(f.read_le_i32()),
            vres: try!(f.read_le_i32()),
            num_colors: try!(f.read_le_u32()),
            num_imp_colors: try!(f.read_le_u32()),
        };

        if dib_header.bits_per_pixel != 24 {
            return Err(BmpError::UnsupportedBitsPerPixel(
                format!("Expected 24, but was {}", dib_header.bits_per_pixel)));
        }

        let row_size = ((24.0 * dib_header.width as f32 + 31.0) / 32.0).floor() as u32 * 4;
        let pixel_array_size = row_size * dib_header.height as u32;
        if pixel_array_size != dib_header.data_size {
            return Err(BmpError::IncorrectDataSize(
                format!("Expected {}, but was {}", pixel_array_size, dib_header.data_size)))
        }

        Ok(dib_header)
    }

    fn read_image_data(f: &mut File, dh: &BmpDibHeader, offset: u32, padding: i64) ->
                       BmpResult<Vec<Pixel>> {
        let mut data = Vec::with_capacity(dh.data_size as usize);
        // seek until data
        try!(f.seek(offset as i64, SeekSet));
        // read pixels until padding
        for _ in (0 .. dh.height) {
            for _ in (0 .. dh.width) {
                let [b, g, r] = [
                    try!(f.read_byte()),
                    try!(f.read_byte()),
                    try!(f.read_byte())
                ];
                data.push(Pixel {r: r, g: g, b: b});
            }
            // seek padding
            try!(f.seek(padding, SeekCur));
        }
        Ok(data)
    }
}

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
        let path_wd = Path::new("src/test/rgbw.bmp");
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
        let bmp_img = Image::open("src/test/rgbw.bmp").unwrap();
        verify_test_bmp_image(bmp_img);
    }

    #[test]
    fn can_read_image_data() {
        let mut f = match File::open(&Path::new("src/test/rgbw.bmp")) {
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
        let bmp_img = Image::open("src/test/rgbw.bmp").unwrap();
        assert_eq!(bmp_img.data.len(), 4);

        assert_eq!(bmp_img.get_pixel(0, 0), RED);
        assert_eq!(bmp_img.get_pixel(1, 0), LIME);
        assert_eq!(bmp_img.get_pixel(0, 1), BLUE);
        assert_eq!(bmp_img.get_pixel(1, 1), WHITE);
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
        let result = Image::open("test/bmptestsuite-0.9/valid/1bpp-1x1.bmp");
        match result {
            Err(BmpError::UnsupportedBitsPerPixel(_)) => (/* Expected */),
            _ => panic!("1bpp should not be supported")
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
        let _ = bmp.save("src/test/rgbw_test.bmp");

        let bmp_img = Image::open("src/test/rgbw_test.bmp").unwrap();
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
    fn write_10x10_bmp(b: &mut test::Bencher) {
        let img = Image::new(10, 10);
        b.iter(|| img.save("src/test/bench_test.bmp"));
    }
}
