#![allow(dead_code)]
use std::str::FromStr;

use base64::{prelude::BASE64_STANDARD, Engine as _};
use futures_util::{SinkExt, StreamExt as _};
use http::Uri;
use tokio::net::TcpStream;
use tokio_websockets::{
	ClientBuilder, MaybeTlsStream, Message, WebSocketStream,
};
use types::{
	Content, ServerContent, ServerContentMessage, SetupCompleteMessage,
};

pub mod types;

pub struct GeminiLiveClient {
	client: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl GeminiLiveClient {
	pub async fn connect() -> Result<Self, String> {
		let uri = format!(
            "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContent?key={apiKey}",
            apiKey = std::env::var("GEMINI_API_KEY")
                .map_err(|_| "GEMINI_API_KEY not set")?
        );
		let uri = Uri::from_str(&uri)
			.map_err(|e| format!("Failed to parse URI: {}", e))?;
		let (client, _) = ClientBuilder::from_uri(uri)
			.connect()
			.await
			.map_err(|e| format!("Failed to connect to Gemini: {}", e))?;

		Ok(Self { client })
	}

	pub async fn setup(&mut self) -> Result<(), String> {
		let message = types::OutgoingMessage::Setup {
			model: "models/gemini-2.0-flash-exp".to_string(),
			system_instruction: Content::new_system(
				"You are a discord bot named Blinky.",
			),
			generation_config: Some(types::LiveGenerationConfig {
				response_modalities: vec![types::Modality::Audio],
				speech_config: Some(types::SpeechConfig {
					voice_config: Some(types::VoiceConfig {
						prebuilt_voice_config: Some(
							types::PrebuiltVoiceConfig {
								voice_name: "Aoede".to_string(),
							},
						),
					}),
				}),
			}),
		};

		let _response: SetupCompleteMessage = self.send(message).await?;

		Ok(())
	}

	#[tracing::instrument(skip(self))]
	async fn send<
		T: serde::Serialize + std::fmt::Debug,
		R: serde::de::DeserializeOwned + std::fmt::Debug,
	>(
		&mut self,
		message: T,
	) -> Result<R, String> {
		let message = serde_json::to_string(&message)
			.map_err(|e| format!("Failed to serialize message: {}", e))?;

		self.client
			.send(Message::text(message))
			.await
			.map_err(|e| format!("Failed to send message: {}", e))?;

		let message = self
			.client
			.next()
			.await
			.ok_or("Failed to receive message")?
			.map_err(|e| format!("Failed to receive message: {}", e))?;

		if message.is_close() {
			tracing::error!(
				"Connection closed: {}",
				message.as_close().unwrap().1
			);
			return Err("Connection closed".to_string());
		}

		let message: &[u8] = message.as_payload();

		let message: R = serde_json::from_slice(message)
			.map_err(|e| format!("Failed to deserialize message: {}", e))?;

		tracing::info!("Received message: {:?}", message);

		Ok(message)
	}

	#[tracing::instrument(skip(self))]
	pub async fn send_realtime_audio(
		&mut self,
		audio_data: Vec<u8>,
		mime_type: &str,
	) -> Result<ServerContent, String> {
		let message = types::OutgoingMessage::ClientContent {
			turns: vec![types::Content {
				role: Some(types::Role::User),
				parts: vec![types::Part::InlineData(
					types::GenerativeContentBlob {
						data: BASE64_STANDARD.encode(&audio_data),
						mime_type: mime_type.to_string(),
					},
				)],
			}],
			turn_complete: true,
		};

		let message: ServerContentMessage = self.send(message).await?;

		Ok(message.server_content)
	}
}
