use bytes::{Buf, BufMut};
use crate::set_vec_len;

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
                println!("VarInt is too big");
                return None;
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
        let mut val = num as u32;

        // adapted from velocity
        if val & (0xFFFFFFFF << 7) == 0 {
            self.put_u8(val as u8);
        } else if val & (0xFFFFFFFF << 14) == 0 {
            let w = (val & 0x7F | 0x80) << 8 | (val >> 7);
            self.put_u16(w as u16);
        } else if val & (0xFFFFFFFF << 21) == 0 {
            let w = (val & 0x7F | 0x80) << 16 | ((val >> 7) & 0x7F | 0x80) << 8 | (val >> 14);
            self.put_uint(w as u64, 3);
        } else {
            // fall back on loop
            loop {
                if val & 0xFFFFFF80 == 0 {
                    self.put_u8(val as u8);
                    return;
                }
                self.put_u8(val as u8 & 0x7F | 0x80);
                val >>= 7;
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