use gemini::GeminiLiveClient;

mod gemini;

pub enum Backend {
	Gemini(GeminiLiveClient),
}

impl Backend {
	pub async fn connect() -> Self {
		let mut backend =
			Backend::Gemini(GeminiLiveClient::connect().await.unwrap());

		match backend {
			Backend::Gemini(ref mut client) => {
				client.setup().await.unwrap();
			}
		}

		backend
	}
}
