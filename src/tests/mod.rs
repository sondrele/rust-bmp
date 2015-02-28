extern crate test;

use std::mem::size_of;
use std::old_io::{File, SeekSet};
use std::old_io::fs::PathExtensions;

use {open, B, M, BmpError, BmpId, BmpHeader, BmpDibHeader, Image, Pixel};
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
    let bmp_img = open("test/rgbw.bmp").unwrap();
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
    let bmp_img = open("test/rgbw.bmp").unwrap();
    assert_eq!(bmp_img.data.len(), 4);

    assert_eq!(bmp_img.get_pixel(0, 0), RED);
    assert_eq!(bmp_img.get_pixel(1, 0), LIME);
    assert_eq!(bmp_img.get_pixel(0, 1), BLUE);
    assert_eq!(bmp_img.get_pixel(1, 1), WHITE);
}

#[test]
fn read_write_1pbb_bmp_image() {
    let img = open("test/bmptestsuite-0.9/valid/1bpp-1x1.bmp").unwrap();
    assert_eq!(img.data.len(), 1);
    assert_eq!(img.get_pixel(0, 0), consts::BLACK);

    let _ = img.save("test/1bb-1x1.bmp");
    let img = open("test/1bb-1x1.bmp").unwrap();
    assert_eq!(img.data.len(), 1);
    assert_eq!(img.get_pixel(0, 0), consts::BLACK);
}

#[test]
fn read_write_4pbb_bmp_image() {
    let img = open("test/bmptestsuite-0.9/valid/4bpp-1x1.bmp").unwrap_or_else(|e| {
        panic!("{}", e);
    });
    assert_eq!(img.data.len(), 1);
    assert_eq!(img.get_pixel(0, 0), consts::BLUE);

    let _ = img.save("test/4bb-1x1.bmp");
    let img = open("test/4bb-1x1.bmp").unwrap_or_else(|e| {
        panic!("{}", e);
    });
    assert_eq!(img.data.len(), 1);
    assert_eq!(img.get_pixel(0, 0), consts::BLUE);
}

#[test]
fn read_write_8pbb_bmp_image() {
    let img = open("test/bmptestsuite-0.9/valid/8bpp-1x1.bmp").unwrap_or_else(|e| {
        panic!("{}", e);
    });
    assert_eq!(img.data.len(), 1);
    assert_eq!(img.get_pixel(0, 0), consts::BLUE);

    let _ = img.save("test/8bb-1x1.bmp");
    let img = open("test/8bb-1x1.bmp").unwrap_or_else(|e| {
        panic!("{}", e);
    });
    assert_eq!(img.data.len(), 1);
    assert_eq!(img.get_pixel(0, 0), consts::BLUE);
}

#[test]
fn error_when_opening_unexisting_image() {
    let result = open("test/no_img.bmp");
    match result {
        Err(BmpError::IoError(_)) => (/* Expected */),
        _ => panic!("Ghost image!?")
    }
}

#[test]
fn error_when_opening_image_with_wrong_bits_per_pixel() {
    let result = open("test/bmptestsuite-0.9/valid/32bpp-1x1.bmp");
    match result {
        Err(BmpError::UnsupportedBitsPerPixel(_)) => (/* Expected */),
        _ => panic!("32bpp should not be supported")
    }
}

#[test]
fn error_when_opening_image_with_wrong_magic_numbers() {
    let result = open("test/bmptestsuite-0.9/corrupt/magicnumber-bad.bmp");
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

    let bmp_img = open("test/rgbw_test.bmp").unwrap();
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
    b.iter(|| open("test/bmptestsuite-0.9/valid/24bpp-320x240.bmp"));
}