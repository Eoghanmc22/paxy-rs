use bytes::BytesMut;
use std::any::Any;

pub mod handling;
pub mod c2s;
pub mod s2c;

pub trait Packet : Any {
    fn read(buffer: &mut BytesMut) -> Self where Self: Sized;
    fn write(&self, buffer: &mut BytesMut);
    fn get_id() -> i32 where Self: Sized;
    fn is_inbound() -> bool where Self: Sized;
}