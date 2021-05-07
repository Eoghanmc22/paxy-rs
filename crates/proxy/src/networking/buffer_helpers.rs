use std::io::{ErrorKind, Read, Write};

use bytes::BufMut;
use mio::net::TcpStream;

use utils::contexts::ConnectionContext;
use utils::indexed_vec::IndexedVec;

pub fn read_socket(ctx: &mut ConnectionContext, packet: &mut IndexedVec<u8>) -> bool {
    let w_i = packet.get_writer_index();
    let result = ctx.stream.read(&mut packet.vec[w_i as usize..]);
    match result {
        Ok(read) => {
            packet.advance_writer_index(read);
            if read == 0 && packet.vec.len() > packet.get_writer_index() {
                println!("read 0");
                ctx.should_close = true;
                return true;
            }
            true
        }
        Err(e) => {
            match e.kind() {
                ErrorKind::WouldBlock => {}
                _ => {
                    println!("unable to read socket: {:?}", e);
                    ctx.should_close = true;
                    return true;
                }
            }
            false
        }
    }
}

pub fn write_socket(ctx: &mut ConnectionContext, packet: &mut IndexedVec<u8>) {
    if ctx.is_writable {
        if !write_socket0(&mut ctx.stream, packet, &mut ctx.should_close) {
            buffer_write(ctx, packet);
            ctx.is_writable = false;
        }
    } else {
        buffer_write(ctx, packet);
    }
}

pub fn write_socket_slice(ctx: &mut ConnectionContext, packet: &[u8]) {
    if ctx.is_writable {
        let mut total_written = 0;
        loop {
            let result = ctx.stream.write(&packet[total_written..]);
            match result {
                Ok(written) => {
                    total_written += written;
                }
                Err(e) => {
                    match e.kind() {
                        ErrorKind::WouldBlock => {
                            buffer_write_slice(ctx, packet, total_written);
                            ctx.is_writable = false;
                            break;
                        }
                        _ => {
                            println!("unable to write socket: {:?}", e);
                            ctx.should_close = true;
                            return;
                        }
                    }
                }
            }
            if total_written >= packet.len() {
                break;
            }
        }
    } else {
        buffer_write_slice(ctx, packet, 0);
    }
}

pub fn write_socket0(stream: &mut TcpStream, packet: &mut IndexedVec<u8>, should_close: &mut bool) -> bool {
    loop {
        let range = packet.get_reader_index()..packet.get_writer_index();
        let result = stream.write(&mut packet.vec[range]);
        match result {
            Ok(written) => {
                packet.advance_reader_index(written);
            }
            Err(e) => {
                match e.kind() {
                    ErrorKind::WouldBlock => {
                        return false;
                    }
                    _ => {
                        println!("unable to write socket: {:?}", e);
                        *should_close = true;
                        return true;
                    }
                }
            }
        }
        if packet.get_reader_index() >= packet.get_writer_index() {
            break;
        }
    }
    true
}

/// store unread data
pub fn buffer_read(ctx: &mut ConnectionContext, buffering_buf: &mut IndexedVec<u8>) {
    let slice = &buffering_buf.vec[buffering_buf.get_reader_index()..buffering_buf.get_writer_index()];
    ctx.read_buffering.put_slice(slice);
}

/// recall unread data
pub fn unbuffer_read(ctx: &mut ConnectionContext, buffering_buf: &mut IndexedVec<u8>) {
    let slice = &ctx.read_buffering.vec[ctx.read_buffering.get_reader_index()..ctx.read_buffering.get_writer_index()];
    buffering_buf.put_slice(slice);
    ctx.read_buffering.reset();
}

/// store unwritten data
pub fn buffer_write(ctx: &mut ConnectionContext, buffering_buf: &mut IndexedVec<u8>) {
    let slice = &buffering_buf.vec[buffering_buf.get_reader_index()..buffering_buf.get_writer_index()];
    ctx.write_buffering.put_slice(slice);
}

/// store unwritten slice of data
pub fn buffer_write_slice(ctx: &mut ConnectionContext, buffering_buf: &[u8], start: usize) {
    let slice = &buffering_buf[start..];
    ctx.write_buffering.put_slice(slice);
}

/// recall unwritten data
pub fn unbuffer_write(ctx: &mut ConnectionContext, buffering_buf: &mut IndexedVec<u8>) {
    let slice = &ctx.write_buffering.vec[ctx.write_buffering.get_reader_index()..ctx.write_buffering.get_writer_index()];
    buffering_buf.put_slice(slice);
    ctx.write_buffering.reset();
}

/// copy data from slice to an IndexedVec
pub fn copy_slice_to(from: &[u8], to: &mut IndexedVec<u8>) {
    let slice = &from[..];
    to.put_slice(slice);
}

pub fn validate_small_frame(buf: &mut IndexedVec<u8>, pointer: usize, len: usize) -> bool {
    if len > pointer && len - pointer < 3 {
        for index in pointer..len {
            if buf.vec[index] < 128 {
                return true;
            }
        }
    }
    false
}
