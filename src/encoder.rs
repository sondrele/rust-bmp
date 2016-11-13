extern crate byteorder;
use byteorder::{LittleEndian, WriteBytesExt};

use std::io::{self, Write};

use Image;

const B: u8 = 66;
const M: u8 = 77;

pub fn encode_image(bmp_image: &Image) -> io::Result<Vec<u8>> {
    let mut bmp_data = Vec::with_capacity(bmp_image.header.file_size as usize);

    write_header(&mut bmp_data, bmp_image)?;
    write_data(&mut bmp_data, bmp_image)?;
    Ok(bmp_data)
}

fn write_header(bmp_data: &mut Vec<u8>, img: &Image) -> io::Result<()> {
    let header = &img.header;
    let dib_header = &img.dib_header;
    let (header_size, data_size) = file_size!(24, img.width, img.height);

    io::Write::write(bmp_data, &[B, M])?;

    bmp_data.write_u32::<LittleEndian>(header_size + data_size)?;
    bmp_data.write_u16::<LittleEndian>(header.creator1)?;
    bmp_data.write_u16::<LittleEndian>(header.creator2)?;
    bmp_data.write_u32::<LittleEndian>(header_size)?; // pixel_offset

    bmp_data.write_u32::<LittleEndian>(dib_header.header_size)?;
    bmp_data.write_i32::<LittleEndian>(dib_header.width)?;
    bmp_data.write_i32::<LittleEndian>(dib_header.height)?;
    bmp_data.write_u16::<LittleEndian>(1)?;  // num_planes
    bmp_data.write_u16::<LittleEndian>(24)?; // bits_per_pixel
    bmp_data.write_u32::<LittleEndian>(0)?;  // compress_type
    bmp_data.write_u32::<LittleEndian>(data_size)?;
    bmp_data.write_i32::<LittleEndian>(dib_header.hres)?;
    bmp_data.write_i32::<LittleEndian>(dib_header.vres)?;
    bmp_data.write_u32::<LittleEndian>(0)?; // num_colors
    bmp_data.write_u32::<LittleEndian>(0)?; // num_imp_colors
    Ok(())
}

fn write_data(bmp_data: &mut Vec<u8>, img: &Image) -> io::Result<()> {
    let padding = &[0; 4][0 .. img.padding as usize];
    for y in 0 .. img.height {
        for x in 0 .. img.width {
            let index = (y * img.width + x) as usize;
            let px = &img.data[index];
            Write::write(bmp_data, &[px.b, px.g, px.r])?;
        }
        Write::write(bmp_data, padding)?;
    }
    Ok(())
}
