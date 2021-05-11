use std::collections::HashMap;

use bytes::Buf;
use utils::contexts::{NetworkThreadContext, ConnectionContext};
use utils::Packet;
use utils::indexed_vec::IndexedVec;
use utils::buffers::VarIntsMut;
use crate::TransformationResult;
use crate::TransformationResult::{Unchanged, Canceled, Modified};

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
    inbound_packets: HashMap<u8, HashMap<i32, Box<dyn Fn(&mut dyn Buf) -> (Box<dyn Packet>, i32) + Send + Sync>>>,
    outbound_packets: HashMap<u8, HashMap<i32, Box<dyn Fn(&mut dyn Buf) -> (Box<dyn Packet>, i32) + Send + Sync>>>,

    inbound_transformers: HashMap<u8, HashMap<i32, Vec<Box<dyn Fn(&mut NetworkThreadContext, &mut ConnectionContext, &mut ConnectionContext, &mut dyn Packet) -> TransformationResult + Send + Sync>>>>,
    outbound_transformers: HashMap<u8, HashMap<i32, Vec<Box<dyn Fn(&mut NetworkThreadContext, &mut ConnectionContext, &mut ConnectionContext, &mut dyn Packet) -> TransformationResult + Send + Sync>>>>,
}

impl HandlingContext {
    pub fn new() -> HandlingContext {
        let mut ctx = HandlingContext {
            inbound_packets: HashMap::new(),
            outbound_packets: HashMap::new(),
            inbound_transformers: HashMap::new(),
            outbound_transformers: HashMap::new()
        };

        for state in 0..=3 {
            ctx.inbound_packets.insert(state, HashMap::new());
            ctx.outbound_packets.insert(state, HashMap::new());
            ctx.inbound_transformers.insert(state, HashMap::new());
            ctx.outbound_transformers.insert(state, HashMap::new());
        }

        ctx
    }

    pub fn handle_packet(&self, thread_ctx: &mut NetworkThreadContext, connection_ctx: &mut ConnectionContext, other_ctx: &mut ConnectionContext, mut packet: UnparsedPacket<&[u8]>, inbound: bool) -> (TransformationResult, Option<IndexedVec<u8>>) {
        let id = packet.id;
        let packet_supplier;
        let transformers;
        if inbound {
            packet_supplier = if let Some(t) = self.inbound_packets.get(&connection_ctx.state).unwrap().get(&id) {
                t
            } else { return (Unchanged, None); };
            transformers = if let Some(t) = self.inbound_transformers.get(&connection_ctx.state).unwrap().get(&id) {
                t
            } else { return (Unchanged, None); };
        } else {
            packet_supplier = if let Some(t) = self.outbound_packets.get(&connection_ctx.state).unwrap().get(&id) {
                t
            } else { return (Unchanged, None); };
            transformers = if let Some(t) = self.outbound_transformers.get(&connection_ctx.state).unwrap().get(&id) {
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
        let packet_id = P::get_id();
        if P::is_inbound() {
            self.inbound_packets.get_mut(&P::get_state()).unwrap().insert(packet_id, Box::new(move |buf| (Box::new(transformer(buf)), P::get_id())));
        } else {
            self.outbound_packets.get_mut(&P::get_state()).unwrap().insert(packet_id, Box::new(move |buf| (Box::new(transformer(buf)), P::get_id())));
        }
    }

    pub fn register_transformer<P: Packet, F: 'static + Fn(&mut NetworkThreadContext, &mut ConnectionContext, &mut ConnectionContext, &mut P) -> TransformationResult + Send + Sync>(&mut self, transformer: F) {
        let packet_id = P::get_id();

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
            if let None = self.inbound_packets.get_mut(&P::get_state()).unwrap().get(&packet_id) {
                self.register_packet_supplier(|buf| {
                   P::read(buf)
                });
            }
            if let Some(vec) = self.inbound_transformers.get_mut(&P::get_state()).unwrap().get_mut(&packet_id) {
                vec.push(transformer);
            } else {
                self.inbound_transformers.get_mut(&P::get_state()).unwrap().insert(packet_id, vec![transformer]);
            }
        } else {
            if let None = self.outbound_packets.get_mut(&P::get_state()).unwrap().get(&packet_id) {
                self.register_packet_supplier(|buf| {
                    P::read(buf)
                });
            }
            if let Some(vec) = self.outbound_transformers.get_mut(&P::get_state()).unwrap().get_mut(&packet_id) {
                vec.push(transformer);
            } else {
                self.outbound_transformers.get_mut(&P::get_state()).unwrap().insert(packet_id, vec![transformer]);
            }
        }
    }
}