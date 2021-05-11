use bytes::Buf;
use utils::contexts::{NetworkThreadContext, ConnectionContext};
use utils::Packet;
use utils::indexed_vec::IndexedVec;
use utils::buffers::VarIntsMut;
use crate::TransformationResult;
use crate::TransformationResult::{Unchanged, Canceled, Modified};

const PACKET_IDS: usize = 0x5B+1;
const STATES: usize = 4;

/// Represents a packet that is decompressed, decrypted, and has a known id.
pub struct UnparsedPacket<T: Buf> {
    id: i32,
    buf: T,
}

impl<T: Buf> UnparsedPacket<T> {
    pub fn new(id: i32, buf: T) -> UnparsedPacket<T> {
        UnparsedPacket { id, buf }
    }
}

/// Contains protocol mapping.
pub struct HandlingContext {
    inbound_packets: [[Option<Box<dyn Fn(&mut dyn Buf) -> (Box<dyn Packet>, i32) + Send + Sync>>; PACKET_IDS]; STATES],
    outbound_packets: [[Option<Box<dyn Fn(&mut dyn Buf) -> (Box<dyn Packet>, i32) + Send + Sync>>; PACKET_IDS]; STATES],

    inbound_transformers: [[Option<Vec<Box<dyn Fn(&mut NetworkThreadContext, &mut ConnectionContext, &mut ConnectionContext, &mut dyn Packet) -> TransformationResult + Send + Sync>>>; PACKET_IDS]; STATES],
    outbound_transformers: [[Option<Vec<Box<dyn Fn(&mut NetworkThreadContext, &mut ConnectionContext, &mut ConnectionContext, &mut dyn Packet) -> TransformationResult + Send + Sync>>>; PACKET_IDS]; STATES],
}

impl HandlingContext {
    pub fn new() -> HandlingContext {
        const NONE1: Option<Box<dyn Fn(&mut dyn Buf) -> (Box<dyn Packet>, i32) + Send + Sync>> = None;
        const NONE2: Option<Vec<Box<dyn Fn(&mut NetworkThreadContext, &mut ConnectionContext, &mut ConnectionContext, &mut dyn Packet) -> TransformationResult + Send + Sync>>> = None;
        const ARRAY1: [Option<Box<dyn Fn(&mut dyn Buf) -> (Box<dyn Packet>, i32) + Send + Sync>>; PACKET_IDS] = [NONE1; PACKET_IDS];
        const ARRAY2: [Option<Vec<Box<dyn Fn(&mut NetworkThreadContext, &mut ConnectionContext, &mut ConnectionContext, &mut dyn Packet) -> TransformationResult + Send + Sync>>>; PACKET_IDS] = [NONE2; PACKET_IDS];

        HandlingContext {
            inbound_packets: [ARRAY1; STATES],
            outbound_packets: [ARRAY1; STATES],
            inbound_transformers: [ARRAY2; STATES],
            outbound_transformers: [ARRAY2; STATES]
        }
    }

    pub fn handle_packet(&self, thread_ctx: &mut NetworkThreadContext, connection_ctx: &mut ConnectionContext, other_ctx: &mut ConnectionContext, mut packet: UnparsedPacket<&[u8]>, inbound: bool) -> (TransformationResult, Option<IndexedVec<u8>>) {
        let id = packet.id as usize;
        let packet_supplier;
        let transformers;

        let state = connection_ctx.state as usize;

        // No such packet
        if state >= STATES && id >= PACKET_IDS {
            println!("No such packet, state: {}, id: {}", connection_ctx.state, id);
            return (Unchanged, None);
        }
        if inbound {
            packet_supplier = if let Some(t) = &self.inbound_packets[state][id] {
                t
            } else { return (Unchanged, None); };
            transformers = if let Some(t) = &self.inbound_transformers[state][id] {
                t
            } else { return (Unchanged, None); };
        } else {
            packet_supplier = if let Some(t) = &self.outbound_packets[state][id] {
                t
            } else { return (Unchanged, None); };
            transformers = if let Some(t) = &self.outbound_transformers[state][id] {
                t
            } else { return (Unchanged, None); };
        }

        let mut packet: (Box<dyn Packet>, i32) = packet_supplier(&mut packet.buf);
        let mut result = Unchanged;

        for transformer in transformers.iter() {
            if result.combine(transformer(thread_ctx, connection_ctx, other_ctx, &mut *packet.0)) {
                return (Canceled, None);
            }
        }

        match result {
            Unchanged => {
                return (Unchanged, None);
            }
            _ => {}
        }

        let mut buffer: IndexedVec<u8> = IndexedVec::new();
        buffer.put_var_i32(packet.1);
        packet.0.write(&mut buffer);

        (Modified, Some(buffer))
    }

    pub fn register_packet_supplier<P: Packet, F: 'static + Fn(&mut dyn Buf) -> P + Send + Sync>(&mut self, transformer: F) {
        let packet_id = P::get_id() as usize;
        let state = P::get_state() as usize;
        if P::is_inbound() {
            self.inbound_packets[state][packet_id] = Some(Box::new(move |buf| (Box::new(transformer(buf)), P::get_id())));
        } else {
            self.outbound_packets[state][packet_id] = Some(Box::new(move |buf| (Box::new(transformer(buf)), P::get_id())));
        }
    }

    pub fn register_transformer<P: Packet, F: 'static + Fn(&mut NetworkThreadContext, &mut ConnectionContext, &mut ConnectionContext, &mut P) -> TransformationResult + Send + Sync>(&mut self, transformer: F) {
        let packet_id = P::get_id() as usize;
        let state = P::get_state() as usize;

        let transformer : Box<dyn Fn(&mut NetworkThreadContext, &mut ConnectionContext, &mut ConnectionContext, &mut dyn Packet) -> TransformationResult + Send + Sync> =
            Box::new(move |thread_ctx, connection_ctx, other_ctx, packet| {
            let any_packet = packet.as_any();
            if let Some(casted_packet) = any_packet.downcast_mut() {
                transformer(thread_ctx, connection_ctx, other_ctx, casted_packet)
            } else {
                println!("couldnt cast, this should never be hit ever");
                Unchanged
            }
        });

        if P::is_inbound() {
            if let None = self.inbound_packets[state][packet_id] {
                self.register_packet_supplier(|buf| {
                   P::read(buf)
                });
            }
            if let Some(vec) = &mut self.inbound_transformers[state][packet_id] {
                vec.push(transformer);
            } else {
                self.inbound_transformers[state][packet_id] = Some(vec![transformer]);
            }
        } else {
            if let None = self.outbound_packets[state][packet_id] {
                self.register_packet_supplier(|buf| {
                    P::read(buf)
                });
            }
            if let Some(vec) = &mut self.outbound_transformers[state][packet_id] {
                vec.push(transformer);
            } else {
                self.outbound_transformers[state][packet_id] = Some(vec![transformer]);
            }
        }
    }
}