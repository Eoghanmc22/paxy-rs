use bytes::{BytesMut, Buf, BufMut};

pub trait VarInts {
    fn get_var_i32(&mut self) -> (i32, i32);

    fn get_var_i32_limit(&mut self, max_size: u32) -> Option<(i32, i32)>;

    fn get_var_i64(&mut self) -> (i64, i64);

    fn put_var_i32(&mut self, num: i32);

    fn put_var_i64(&mut self, num: i64);
}

impl VarInts for BytesMut {
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

    fn put_bool(&mut self, val: bool);
}

impl Bools for BytesMut {
    fn get_bool(&mut self) -> bool {
        self.get_u8() != 0
    }

    fn put_bool(&mut self, val: bool) {
        self.put_u8(if val { 1 } else { 0 });
    }
}