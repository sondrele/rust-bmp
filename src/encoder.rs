extern crate byteorder;
use byteorder::{LittleEndian, WriteBytesExt};

use std::io::{self, Write};

use Image;

const B: u8 = 66;
const M: u8 = 77;

pub fn encode_image(bmp_image: &Image) -> io::Result<Vec<u8>> {
    let mut bmp_data = Vec::with_capacity(bmp_image.header.file_size as usize);

    try!(write_header(&mut bmp_data, bmp_image));
    try!(write_data(&mut bmp_data, bmp_image));
    Ok(bmp_data)
}

fn write_header(bmp_data: &mut Vec<u8>, img: &Image) -> io::Result<()> {
    let header = &img.header;
    let dib_header = &img.dib_header;
    let (header_size, data_size) = file_size!(24, img.width, img.height);

    try!(io::Write::write(bmp_data, &[B, M]));

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

fn write_data(bmp_data: &mut Vec<u8>, img: &Image) -> io::Result<()> {
    let padding = &[0; 4][0 .. img.padding as usize];
    for y in 0 .. img.height {
        for x in 0 .. img.width {
            let index = (y * img.width + x) as usize;
            let px = &img.data[index];
            try!(Write::write(bmp_data, &[px.b, px.g, px.r]));
        }
        try!(Write::write(bmp_data, padding));
    }
    Ok(())
}
