use std::io::{ErrorKind, Read, Write};

use bytes::{Buf, BufMut};
use bytes::buf::UninitSlice;
use mio::net::TcpStream;

use crate::contexts::ConnectionContext;

pub trait VarInts {
    fn get_var_i32(&mut self) -> (i32, i32);

    fn get_var_i32_limit(&mut self, max_size: u32) -> Option<(i32, i32)>;

    fn get_var_i64(&mut self) -> (i64, i64);
}

pub trait VarIntsMut {
    fn put_var_i32(&mut self, num: i32);

    fn put_var_i64(&mut self, num: i64);
}

impl<T: Buf> VarInts for T {
    fn get_var_i32(&mut self) -> (i32, i32) {
        let mut num_read = 0i32;
        let mut result = 0i32;
        let mut read;
        loop {
            read = self.get_u8() as i32;
            result |= (read & 0b01111111).overflowing_shl((7 * num_read) as u32).0;
            num_read += 1;
            if num_read > 5 {
                panic!("VarInt is too big")
            }
            if read & 0b10000000 == 0 {
                break;
            }
        }
        (result, num_read)
    }

    fn get_var_i32_limit(&mut self, max_size: u32) -> Option<(i32, i32)> {
        let mut num_read = 0i32;
        let mut result = 0i32;
        let mut read;
        while num_read < max_size as i32 {
            read = self.get_u8() as i32;
            result |= (read & 0b01111111).overflowing_shl((7 * num_read) as u32).0;
            num_read += 1;
            if num_read > 5 {
                panic!("VarInt is too big")
            }
            if read & 0b10000000 == 0 {
                return Some((result, num_read));
            }
        }
        None
    }

    fn get_var_i64(&mut self) -> (i64, i64) {
        let mut num_read = 0i64;
        let mut result = 0i64;
        let mut read;
        loop {
            read = self.get_u8() as i64;
            result |= (read & 0b01111111).overflowing_shl((7 * num_read) as u32).0;
            num_read += 1;
            if num_read > 5 {
                panic!("VarInt is too big")
            }
            if read & 0b10000000 == 0 {
                break;
            }
        }
        (result, num_read)
    }
}

impl<T: BufMut> VarIntsMut for T {
    fn put_var_i32(&mut self, num: i32) {
        let mut number = num;
        loop {
            let mut temp = number as u8 & 0b01111111;
            number >>= 7;
            if number != 0 {
                temp |= 0b10000000;
            }
            self.put_u8(temp);
            if number == 0 {
                break;
            }
        }
    }

    fn put_var_i64(&mut self, num: i64) {
        let mut number = num;
        loop {
            let mut temp = number as u8 & 0b01111111;
            number >>= 7;
            if number != 0 {
                temp |= 0b10000000;
            }
            self.put_u8(temp);
            if number == 0 {
                break;
            }
        }
    }
}

pub trait Bools {
    fn get_bool(&mut self) -> bool;
}

pub trait BoolsMut {
    fn put_bool(&mut self, val: bool);
}

impl<T: Buf> Bools for T {
    fn get_bool(&mut self) -> bool {
        self.get_u8() != 0
    }
}

impl<T: BufMut> BoolsMut for T {
    fn put_bool(&mut self, val: bool) {
        self.put_u8(if val { 1 } else { 0 });
    }
}

pub trait Strings {
    fn get_string(&mut self) -> String;
}

pub trait StringsMut {
    fn put_string(&mut self, string: &str);
}

impl<T: Buf> Strings for T {
    fn get_string(&mut self) -> String {
        let len = self.get_var_i32().0;
        let mut slice = Vec::new();
        set_vec_len(&mut slice, len as usize);
        self.copy_to_slice(&mut slice);
        String::from_utf8_lossy(&slice).to_string()
    }
}

impl<T: BufMut> StringsMut for T {
    fn put_string(&mut self, string: &str) {
        //String slices are always valid UTF-8
        self.put_var_i32(string.len() as i32);
        self.put_slice(string.as_bytes());
    }
}

pub fn set_vec_len<T>(vec: &mut Vec<T>, extra_len: usize) {
    vec.reserve(extra_len);

    // SAFETY:
    // This is safe because we will always write the uninitialized memory before reading it
    // and we reserve enough capacity.

    // Reason we dont use a safe method is because this is a lot faster. We dont need to initialize all of it
    // with 0s just to overwrite them.
    unsafe {
        vec.set_len(vec.len()+extra_len);
    }
}

#[derive(Debug)]
pub struct IndexedVec<T> {
    pub vec: Vec<T>,
    writer_index: usize,
    reader_index: usize,
}

impl<T> IndexedVec<T> {
    pub fn new() -> IndexedVec<T> {
        IndexedVec {
            vec: Vec::new(),
            writer_index: 0,
            reader_index: 0
        }
    }

    pub fn from_vec(vec: Vec<T>) -> IndexedVec<T> {
        IndexedVec {
            vec,
            writer_index: 0,
            reader_index: 0
        }
    }

    pub fn get_writer_index(&self) -> usize {
        self.writer_index
    }

    pub fn get_reader_index(&self) -> usize {
        self.reader_index
    }

    pub fn set_writer_index(&mut self, writer_index: usize) {
        self.writer_index = writer_index;
    }

    pub fn set_reader_index(&mut self, writer_index: usize) {
        self.reader_index = writer_index;
    }

    pub fn advance_writer_index(&mut self, distance: usize) {
        self.writer_index += distance;
    }

    pub fn advance_reader_index(&mut self, distance: usize) {
        self.reader_index += distance;
    }

    pub fn reset(&mut self) {
        self.writer_index = 0;
        self.reader_index = 0;
    }

    pub fn reset_reader(&mut self) {
        self.reader_index = 0;
    }

    pub fn reset_writer(&mut self) {
        self.writer_index = 0;
    }

    pub fn ensure_writable(&mut self, extra: usize) {
        let remaining = self.vec.len() - self.get_writer_index();
        if remaining < extra {
            let needed = extra - remaining;
            set_vec_len(&mut self.vec, needed);
        }
    }
}

impl Buf for IndexedVec<u8> {
    fn remaining(&self) -> usize {
        self.vec.len() - self.get_reader_index()
    }

    fn chunk(&self) -> &[u8] {
        &self.vec[self.get_reader_index()..]
    }

    fn advance(&mut self, cnt: usize) {
        self.advance_reader_index(cnt);
        if self.get_reader_index() >= self.vec.len() {
            panic!("no more space, reader_index: {} cnt: {}, len: {}", self.get_reader_index()-cnt, cnt, self.vec.len())
        }
    }
}

unsafe impl BufMut for IndexedVec<u8> {
    fn remaining_mut(&self) -> usize {
        self.vec.len() - self.get_writer_index()
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        self.advance_writer_index(cnt);
        if self.get_writer_index() > self.vec.len() {
            panic!("no more space, writer_index: {} cnt: {}, len: {}", self.get_writer_index()-cnt, cnt, self.vec.len())
        }
    }

    fn chunk_mut(&mut self) -> &mut UninitSlice {
        if self.get_writer_index() == self.vec.len() {
            set_vec_len(&mut self.vec, 64) // Grow the vec
        }

        let cap = self.vec.len();
        let len = self.get_writer_index();

        let ptr = self.vec.as_mut_ptr();
        unsafe { &mut UninitSlice::from_raw_parts_mut(ptr, cap)[len..] }
    }

    fn put_slice(&mut self, src: &[u8]) {
        self.ensure_writable(src.len());

        // default impl
        {
            let mut off = 0;

            assert!(
                self.remaining_mut() >= src.len(),
                "buffer overflow; remaining = {}; src = {}",
                self.remaining_mut(),
                src.len()
            );

            while off < src.len() {
                let cnt;

                unsafe {
                    let dst = self.chunk_mut();
                    cnt = std::cmp::min(dst.len(), src.len() - off);

                    std::ptr::copy_nonoverlapping(src[off..].as_ptr(), dst.as_mut_ptr() as *mut u8, cnt);

                    off += cnt;
                }

                unsafe {
                    self.advance_mut(cnt);
                }
            }
        }
    }
}

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

pub fn buffer_read(ctx: &mut ConnectionContext, buffering_buf: &mut IndexedVec<u8>) {
    let slice = &buffering_buf.vec[buffering_buf.get_reader_index()..buffering_buf.get_writer_index()];
    ctx.read_buffering.put_slice(slice);
}

pub fn unbuffer_read(ctx: &mut ConnectionContext, buffering_buf: &mut IndexedVec<u8>) {
    let slice = &ctx.read_buffering.vec[ctx.read_buffering.get_reader_index()..ctx.read_buffering.get_writer_index()];
    buffering_buf.put_slice(slice);
    ctx.read_buffering.reset();
}

pub fn buffer_write(ctx: &mut ConnectionContext, buffering_buf: &mut IndexedVec<u8>) {
    let slice = &buffering_buf.vec[buffering_buf.get_reader_index()..buffering_buf.get_writer_index()];
    ctx.write_buffering.put_slice(slice);
}

pub fn buffer_write_slice(ctx: &mut ConnectionContext, buffering_buf: &[u8], start: usize) {
    let slice = &buffering_buf[start..];
    ctx.write_buffering.put_slice(slice);
}

pub fn unbuffer_write(ctx: &mut ConnectionContext, buffering_buf: &mut IndexedVec<u8>) {
    let slice = &ctx.write_buffering.vec[ctx.write_buffering.get_reader_index()..ctx.write_buffering.get_writer_index()];
    buffering_buf.put_slice(slice);
    ctx.write_buffering.reset();
}

pub fn write_slice(to: &mut IndexedVec<u8>, from: &[u8]) {
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
