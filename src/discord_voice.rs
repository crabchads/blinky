use std::time::SystemTime;

use futures_util::{SinkExt as _, StreamExt as _};
use http::Uri;
use serde::{Deserialize, Serialize};
use tokio::{net::TcpStream, task::JoinHandle};
use tokio_websockets::{
	ClientBuilder, MaybeTlsStream, Message, WebSocketStream,
};
use twilight_gateway::Session;

#[derive(Serialize)]
struct Identify {
	pub op: u8,
	pub d: IdentifyData,
}

#[derive(Serialize)]
struct IdentifyData {
	pub server_id: String,
	pub user_id: String,
	pub session_id: String,
	pub token: String,
}

#[derive(Deserialize)]
struct VoiceReady {
	pub op: u8,
	pub d: VoiceReadyData,
}

#[derive(Deserialize)]
struct VoiceReadyData {
	pub ssrc: u32,
	pub ip: String,
	pub port: u16,
	pub modes: Vec<String>,
	pub heartbeat_interval: u64,
}

#[derive(Serialize)]
struct Heartbeat {
	pub op: u8,
	pub d: HeartbeatData,
}

#[derive(Serialize)]
struct HeartbeatData {
	pub t: u64,
	pub seq_ack: u64,
}

pub struct DiscordVoiceClient {
	client: WebSocketStream<MaybeTlsStream<TcpStream>>,
	pub heartbeat_interval: u64,
}

impl<'a> DiscordVoiceClient {
	pub async fn connect(
		endpoint: String,
		session_id: String,
		token: String,
		guild_id: u64,
		user_id: u64,
	) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
		let uri = Uri::builder()
			.scheme("wss")
			.authority(endpoint)
			.build()
			.map_err(|e| format!("Failed to build URI: {}", e))?;
		let (mut client, _) = ClientBuilder::from_uri(uri)
			.connect()
			.await
			.map_err(|e| format!("Failed to connect to Gemini: {}", e))?;

		let identify = Identify {
			op: 0,
			d: IdentifyData {
				server_id: guild_id.to_string(),
				user_id: user_id.to_string(),
				session_id: session_id.to_string(),
				token: token.to_string(),
			},
		};

		let identify = serde_json::to_string(&identify)
			.map_err(|e| format!("Failed to serialize identify: {}", e))?;

		client.send(Message::text(identify)).await?;

		// Receive the voice ready event
		let message =
			client
				.next()
				.await
				.ok_or("Failed to receive message")?
				.map_err(|e| format!("Failed to receive message: {}", e))?;

		let voice_ready: VoiceReady = serde_json::from_str(
			message.as_text().as_ref().unwrap(),
		)
		.map_err(|e| format!("Failed to deserialize voice ready: {}", e))?;
		if voice_ready.op != 2 {
			return Err("Expected voice ready event".into());
		}

		Ok(Self {
			client,
			heartbeat_interval: voice_ready.d.heartbeat_interval,
		})
	}

	pub async fn heartbeat(&mut self, sequence: u64) {
		tokio::time::sleep(std::time::Duration::from_millis(
			self.heartbeat_interval,
		))
		.await;
		self.client
			.send(Message::text(
				serde_json::to_string(&Heartbeat {
					op: 3,
					d: HeartbeatData {
						t: SystemTime::now()
							.duration_since(SystemTime::UNIX_EPOCH)
							.unwrap()
							.as_millis() as u64,
						seq_ack: sequence,
					},
				})
				.unwrap(),
			))
			.await
			.unwrap();
	}
}
