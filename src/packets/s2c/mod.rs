use crate::packets::Packet;
use bytes::{BytesMut, BufMut, Buf};
use crate::utils::{VarInts, Bools};

pub struct EntityPositionPacket {
    pub entity_id: i32,
    pub delta_x: i16,
    pub delta_y: i16,
    pub delta_z: i16,
    pub on_ground: bool
}
impl Packet for EntityPositionPacket {
    fn read(buffer: &mut BytesMut) -> Self {
        let entity_id = buffer.get_var_i32().0;
        let delta_x = buffer.get_i16();
        let delta_y = buffer.get_i16();
        let delta_z = buffer.get_i16();
        let on_ground = buffer.get_bool();
        EntityPositionPacket {
            entity_id,
            delta_x,
            delta_y,
            delta_z,
            on_ground
        }
    }

    fn write(&self, buffer: &mut BytesMut) {
        buffer.put_var_i32(self.entity_id);
        buffer.put_i16(self.delta_x);
        buffer.put_i16(self.delta_y);
        buffer.put_i16(self.delta_z);
        buffer.put_bool(self.on_ground);
    }

    fn get_id() -> i32 where Self: Sized {
        0x27
    }

    fn is_inbound() -> bool where Self: Sized {
        false
    }
}