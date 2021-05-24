use crate::{add_vec_len, set_vec_len};
use bytes::{Buf, BufMut};
use bytes::buf::UninitSlice;
use std::io::{Write, Error, Read};

#[derive(Debug)]
pub struct IndexedVec<T> {
    pub vec: Vec<T>,
    writer_index: usize,
    reader_index: usize,
}

impl<T> IndexedVec<T> {
    pub fn new() -> IndexedVec<T> {
        IndexedVec::with_len(256)
    }

    pub fn with_len(len: usize) -> IndexedVec<T> {
        let mut vec = Vec::with_capacity(len);
        set_vec_len(&mut vec, len);
        IndexedVec::from_vec(vec)
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
            self.reallocate(needed);
        }
    }

    pub fn reallocate(&mut self, min_extra: usize) {
        let mut extra = 0;
        let increment = self.vec.len();
        while extra < min_extra {
            extra += increment;
            add_vec_len(&mut self.vec, increment) // Grow the vec
        }
    }

    pub fn as_slice(&self) -> &[T] {
        &self.vec[self.get_reader_index()..self.get_writer_index()]
    }

    pub fn as_mut_write_slice(&mut self) -> &mut [T] {
        let range = self.get_writer_index()..;
        &mut self.vec[range]
    }

    pub fn readable_bytes(&self) -> usize {
        self.writer_index-self.reader_index
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
        debug_assert!(self.get_reader_index() > self.vec.len(), "no more space, reader_index: {} cnt: {}, len: {}", self.get_reader_index()-cnt, cnt, self.vec.len());
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
            self.reallocate(64); // Grow the vec
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

impl Write for IndexedVec<u8> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.put_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

impl Read for IndexedVec<u8> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.copy_to_slice(buf);
        Ok(buf.len())
    }
}