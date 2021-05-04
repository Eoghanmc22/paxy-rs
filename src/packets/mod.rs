use bytes::{Buf, BufMut};
use std::any::Any;

pub mod handling;
pub mod c2s;
pub mod s2c;

pub trait Packet : Any {
    fn read(buffer: &mut dyn Buf) -> Self where Self: Sized;
    fn write(&self, buffer: &mut dyn BufMut);
    fn get_id() -> i32 where Self: Sized;
    fn is_inbound() -> bool where Self: Sized;
    fn as_any(&mut self) -> &mut dyn Any;
}