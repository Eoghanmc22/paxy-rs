use std::any::Any;

use bytes::{Buf, BufMut};

pub mod buffers;
pub mod indexed_vec;
pub mod sendable;
pub mod contexts;
pub mod buffer_helpers;

pub fn add_vec_len<T>(vec: &mut Vec<T>, extra_len: usize) {
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

pub fn set_vec_len<T>(vec: &mut Vec<T>, len: usize) {
    if len > vec.len() {
        vec.reserve(len-vec.len());
    }

    // SAFETY:
    // This is safe because we will always write the uninitialized memory before reading it
    // and we reserve enough capacity.

    // Reason we dont use a safe method is because this is a lot faster. We dont need to initialize all of it
    // with 0s just to overwrite them.
    unsafe {
        vec.set_len(len);
    }
}

pub fn get_var_i32_size(num : i32) -> i32 {
    let num = num as u64;
    if num & 0xFFFFFF80 == 0 {
        1
    } else if num & 0xFFFFC000 == 0 {
        2
    } else if num & 0xFFE00000 == 0 {
        3
    } else if num & 0xF0000000 == 0 {
        4
    } else {
        5
    }
}

pub trait Packet : Any {
    fn read(buffer: &mut dyn Buf) -> Self
        where Self: Sized;

    fn write(&self, buffer: &mut dyn BufMut);

    fn get_id() -> i32
        where Self: Sized;

    fn get_state() -> u8
        where Self: Sized;

    fn is_inbound() -> bool
        where Self: Sized;

    fn as_any(&mut self) -> &mut dyn Any;
}
