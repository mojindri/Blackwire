#![no_main]

#[path = "common.rs"]
mod common;

use std::io::Cursor;

use blackwire_protocol::vmess::codec::decode_header;
use libfuzzer_sys::fuzz_target;

const CMD_KEY: [u8; 16] = [0x11; 16];
const AUTH_ID: [u8; 16] = [0x22; 16];
const CONNECTION_NONCE: [u8; 8] = [0x33; 8];

fuzz_target!(|data: &[u8]| {
    let data = common::bounded(data, 4096);
    common::block_on(async {
        let mut cursor = Cursor::new(data);
        let _ = decode_header(
            &mut cursor,
            &CMD_KEY,
            &AUTH_ID,
            &CONNECTION_NONCE,
            data.len(),
        )
        .await;
    });
});
