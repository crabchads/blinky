pub mod client;

use base64::{engine::general_purpose, Engine as _};
use rand::Rng;
use sha1::{Digest as _, Sha1};
use std::error::Error;

// Constants for WebSocket handshake
pub(crate) const WEBSOCKET_GUID: &str = "258EAFA5-E914-47DA-95CA-C5B5DA856CE6";

pub(crate) fn generate_websocket_key() -> String {
	let mut rng = rand::rng();
	let key: [u8; 16] = rng.random();

	general_purpose::STANDARD.encode(&key)
}

pub(crate) fn build_websocket_handshake_request(
	host: &str,
	key: &str,
) -> String {
	format!(
		"GET / HTTP/1.1\r\n\
        Host: {}\r\n\
        Connection: Upgrade\r\n\
        Upgrade: websocket\r\n\
        Sec-WebSocket-Key: {}\r\n\
        Sec-WebSocket-Version: 13\r\n\
        \r\n",
		host, key
	)
}

pub(crate) fn calculate_websocket_accept(key: &str) -> String {
	let mut hasher = Sha1::new();
	hasher.update(key.as_bytes());
	hasher.update(WEBSOCKET_GUID.as_bytes());

	general_purpose::STANDARD.encode(&hasher.finalize())
}

pub(crate) fn build_websocket_frame(message: &str, is_text: bool) -> Vec<u8> {
	let mut frame = Vec::new();

	// FIN, RSV1, RSV2, RSV3, opcode
	let opcode = if is_text { 0x1 } else { 0x2 };
	frame.push(0x80 | opcode);

	// Mask, payload length
	let payload = message.as_bytes();
	let payload_len = payload.len();
	if payload_len < 126 {
		frame.push(0x80 | payload_len as u8);
	} else if payload_len < 65536 {
		frame.push(0x80 | 126);
		frame.push((payload_len >> 8) as u8);
		frame.push(payload_len as u8);
	} else {
		frame.push(0x80 | 127);
		frame.push((payload_len >> 56) as u8);
		frame.push((payload_len >> 48) as u8);
		frame.push((payload_len >> 40) as u8);
		frame.push((payload_len >> 32) as u8);
		frame.push((payload_len >> 24) as u8);
		frame.push((payload_len >> 16) as u8);
		frame.push((payload_len >> 8) as u8);
		frame.push(payload_len as u8);
	}

	// Masking key
	let mut masking_key = [0; 4];
	let mut rng = rand::rng();
	rng.fill(&mut masking_key);
	frame.extend_from_slice(&masking_key);

	// Payload
	for (i, byte) in payload.iter().enumerate() {
		frame.push(byte ^ masking_key[i % 4]);
	}

	frame
}

pub(crate) fn parse_websocket_frame(
	buffer: &[u8],
) -> Result<(Vec<u8>, bool), Box<dyn Error>> {
	let mut offset = 0;

	// FIN, RSV1, RSV2, RSV3, opcode
	let fin = buffer[offset] & 0x80 != 0;
	let _opcode = buffer[offset] & 0x0F;
	offset += 1;

	// Mask, payload length
	let masked = buffer[offset] & 0x80 != 0;
	let payload_len = buffer[offset] & 0x7F;
	offset += 1;

	let payload_len = if payload_len < 126 {
		payload_len as usize
	} else if payload_len == 126 {
		let len = u16::from_be_bytes([buffer[offset], buffer[offset + 1]]);
		offset += 2;
		len as usize
	} else {
		let len = u64::from_be_bytes([
			buffer[offset],
			buffer[offset + 1],
			buffer[offset + 2],
			buffer[offset + 3],
			buffer[offset + 4],
			buffer[offset + 5],
			buffer[offset + 6],
			buffer[offset + 7],
		]);
		offset += 8;
		len as usize
	};

	// Masking key
	let masking_key = if masked {
		let mut key = [0; 4];
		key.copy_from_slice(&buffer[offset..offset + 4]);
		offset += 4;
		key
	} else {
		[0; 4]
	};

	// Payload
	let mut payload = Vec::with_capacity(payload_len);
	for i in 0..payload_len {
		payload.push(buffer[offset + i] ^ masking_key[i % 4]);
	}

	Ok((payload, fin))
}
