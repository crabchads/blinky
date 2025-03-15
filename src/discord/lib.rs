use bssl_sys::*;
use std::error::Error;

pub struct Bot {
	base_url: String,
	token: String,
	ssl: *mut SSL,
}

impl Bot {
	pub fn new(token: &str) -> Self {
		Self {
			base_url: "https://discord.com/api/v10".to_string(),
			token: token.to_string(),
			ssl: std::ptr::null_mut(),
		}
	}

	pub fn connect(&mut self) -> Result<(), Box<dyn Error>> {
		// Initialize the SSL context
		let ctx = unsafe { SSL_CTX_new(TLS_client_method()) };
		if ctx.is_null() {
			return Err("Failed to create SSL context".into());
		}

		self.ssl = unsafe { SSL_new(ctx) };
		if self.ssl.is_null() {
			return Err("Failed to create SSL object".into());
		}

		Ok(())
	}
}

impl Drop for Bot {
	fn drop(&mut self) {
		if !self.ssl.is_null() {
			unsafe {
				SSL_free(self.ssl);
			}
		}
	}
}
