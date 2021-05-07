extern crate proc_macro;
use proc_macro::TokenStream;

use syn::{parse_macro_input, DeriveInput, Data, Fields, Meta::List, NestedMeta::Lit, Lit::{Int, Bool}};
use quote::quote;
use proc_macro2::Ident;

// https://doc.rust-lang.org/reference/procedural-macros.html#derive-mode-macros
// TODO please improve me
#[proc_macro_derive(Packet, attributes(packet))]
pub fn derive_packet(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let data = match input.data {
        Data::Struct(d) => {
           d
        }
        _ => panic!("usage of derive_packet in unsupported place")
    };

    let fields = match data.fields {
        Fields::Named(f) => {
            f
        }
        _ => panic!("usage of derive_packet in unsupported place")
    }.named;

    let mapped_fields : Vec<&Ident> = fields.iter().map(|field| field.ident.as_ref().unwrap()).collect();

    let name = input.ident;
    let mut packet_attr = None;

    for attr in input.attrs.iter() {
        if attr.path.is_ident("packet") {
            packet_attr = Some(attr);
        }
    }

    if packet_attr.is_none() {
        panic!("no attr packet");
    }

    let packet_attr = packet_attr.unwrap();

    let meta = &packet_attr.parse_meta().expect("there is no meta");

    let id;
    let state;
    let inbound;

    match meta {
        List(m) => {
            match &m.nested[0] {
                Lit(l) => {
                    match l {
                        Int(i) => {
                            id = i;
                        },
                        _ => panic!("id isnt an int")
                    }
                }
                _ => panic!("id is not a lit")
            }
            state = &m.nested[1];
            match &m.nested[2] {
                Lit(l)=> {
                    match l {
                        Bool(i) => {
                            inbound = i;
                        },
                        _ => panic!("inbound isnt a bool")
                    }
                }
                _ => panic!("inbound is not a lit")
            }
        },
        _ => panic!("invalid meta")
    }

    let tokens = quote! {
        impl utils::Packet for #name {
            fn read(mut buffer: &mut dyn bytes::Buf) -> Self where Self: Sized {
                #( let #mapped_fields = utils::sendable::Sendable::read(buffer); )*
                #name {
                    #( #mapped_fields ),*
                }
            }

            fn write(&self, mut buffer: &mut dyn bytes::BufMut) {
                #( utils::sendable::Sendable::write(buffer, &self.#mapped_fields); )*
            }

            fn get_id() -> i32 where Self: Sized {
                #id
            }

            fn get_state() -> u8 where Self: Sized {
                #state
            }

            fn is_inbound() -> bool where Self: Sized {
                #inbound
            }

            fn as_any(&mut self) -> &mut dyn std::any::Any {
                self
            }
        }
    };

    tokens.into()
}