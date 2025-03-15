use std::error::Error;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::fd::AsRawFd;

use bssl_sys::*;

use crate::{
	build_websocket_frame, build_websocket_handshake_request,
	calculate_websocket_accept, generate_websocket_key, parse_websocket_frame,
};

unsafe extern "C" {
	fn close(fd: i32) -> i32;
}

pub struct WebSocketClient {
	ssl: *mut SSL,
}

impl WebSocketClient {
	pub fn new(ssl: *mut SSL) -> Self {
		Self { ssl }
	}

	pub fn connect(
		host: &str,
		stream: &mut TcpStream,
	) -> Result<Self, Box<dyn Error>> {
		// Initialize the SSL context
		let ctx = unsafe { SSL_CTX_new(TLS_client_method()) };
		if ctx.is_null() {
			return Err("Failed to create SSL context".into());
		}

		let ssl = unsafe { SSL_new(ctx) };
		if ssl.is_null() {
			return Err("Failed to create SSL object".into());
		}

		let rc = unsafe { SSL_set_fd(ssl, stream.as_raw_fd()) };
		if rc != 1 {
			return Err("Failed to set file descriptor for SSL object".into());
		}

		let rc =
			unsafe { SSL_set_tlsext_host_name(ssl, host.as_ptr() as *const _) };
		if rc != 1 {
			return Err("Failed to set hostname for SSL object".into());
		}

		// Perform the WebSocket handshake
		let key = generate_websocket_key();
		let request = build_websocket_handshake_request(host, &key);
		stream.write_all(request.as_bytes())?;

		// Read the server's response
		let mut response = String::new();
		stream.read_to_string(&mut response)?;

		// Validate the server's response
		let expected_accept = calculate_websocket_accept(&key);
		if !response.contains(&expected_accept) {
			eprintln!(
				"Handshake failed. Expected accept: {}, Response: {}",
				expected_accept, response
			);
			return Err("WebSocket handshake failed".into());
		}

		// Perform the SSL handshake
		let rc = unsafe { SSL_connect(ssl) };
		if rc != 1 {
			let err = unsafe { SSL_get_error(ssl, rc) };
			let err_str = unsafe {
				ERR_error_string(ERR_get_error(), std::ptr::null_mut())
			};
			let err_str_owned = unsafe {
				std::ffi::CStr::from_ptr(err_str)
					.to_string_lossy()
					.into_owned()
			};
			eprintln!(
				"SSL connect failed: error code: {}, error string: {}",
				err, err_str_owned
			);
			return Err("SSL handshake failed".into());
		}

		Ok(Self { ssl })
	}

	pub fn send(&self, message: &str) -> Result<(), Box<dyn Error>> {
		let frame = build_websocket_frame(message, true);
		let sent = unsafe {
			SSL_write(self.ssl, frame.as_ptr() as *const _, frame.len() as i32)
		};

		if sent != frame.len() as i32 {
			eprintln!("Sent {} bytes, expected {}.", sent, frame.len());
			return Err("Failed to send the entire frame".into());
		}

		Ok(())
	}

	pub fn receive(&self) -> Result<(String, bool), Box<dyn Error>> {
		let mut buffer: [u8; 4096] = [0; 4096];
		let received = unsafe {
			SSL_read(
				self.ssl,
				buffer.as_mut_ptr() as *mut _,
				buffer.len() as i32,
			)
		};

		if received > 0 {
			// Parse the WebSocket frame
			let (payload, is_final) =
				parse_websocket_frame(&buffer[..received as usize])?;

			// Print the received message
			let received_message = String::from_utf8_lossy(&payload);

			Ok((received_message.to_string(), is_final))
		} else if received == 0 {
			println!("Connection closed by server");
			Ok(("".to_string(), false))
		} else {
			let err = unsafe { SSL_get_error(self.ssl, received) };
			let err_str = unsafe {
				ERR_error_string(ERR_get_error(), std::ptr::null_mut())
			};
			let err_str_owned = unsafe {
				std::ffi::CStr::from_ptr(err_str)
					.to_string_lossy()
					.into_owned()
			};
			eprintln!(
				"SSL read failed: error code: {}, error string: {}",
				err, err_str_owned
			);
			Err("SSL read failed".into())
		}
	}
}

impl Drop for WebSocketClient {
	fn drop(&mut self) {
		// Cleanup
		unsafe {
			SSL_shutdown(self.ssl);
			SSL_free(self.ssl);
		}
	}
}
