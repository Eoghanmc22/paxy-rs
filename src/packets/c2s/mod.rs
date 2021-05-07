pub mod handshake {
    use crate::packets::{Packet, networking};
    use bytes::{Buf, BufMut};
    use std::any::Any;
    use crate::utils::{VarIntsMut, StringsMut, VarInts, Strings};

    pub struct HandshakePacket {
        pub protocol_version: i32,
        pub ip: String,
        pub port: u16,
        pub next_state: u8
    }

    impl Packet for HandshakePacket {
        fn read(mut buffer: &mut dyn Buf) -> Self where Self: Sized {
            let protocol_version = buffer.get_var_i32().0;
            let ip = buffer.get_string();
            let port = buffer.get_u16();
            let next_state = buffer.get_var_i32().0 as u8;

            HandshakePacket {
                protocol_version,
                ip,
                port,
                next_state
            }
        }

        fn write(&self, mut buffer: &mut dyn BufMut) {
            buffer.put_var_i32(self.protocol_version);
            buffer.put_string(&self.ip);
            buffer.put_u16(self.port);
            buffer.put_var_i32(self.next_state as i32);
        }

        fn get_id() -> i32 where Self: Sized {
            0x00
        }

        fn get_state() -> u8 where Self: Sized {
            networking::HANDSHAKING_STATE
        }

        fn is_inbound() -> bool where Self: Sized {
            true
        }

        fn as_any(&mut self) -> &mut dyn Any {
            self
        }
    }
}