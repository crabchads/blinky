use std::io::Cursor;

use serde::{Deserialize, Serialize};
use songbird::input::{
	core::io::MediaSource, AudioStream, AudioStreamError, Compose,
};

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SetupCompleteMessage {
	pub setup_complete: serde_json::Value,
}

#[derive(Default, Deserialize, Serialize, Debug)]
pub struct Content {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub role: Option<Role>,
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub parts: Vec<Part>,
}

impl Content {
	pub fn new(string: impl ToString) -> Self {
		Self {
			role: Some(Role::User),
			parts: vec![Part::Text(string.to_string())],
		}
	}

	pub fn new_system(string: impl ToString) -> Self {
		Self {
			role: None,
			parts: vec![Part::Text(string.to_string())],
		}
	}

	pub fn is_empty(&self) -> bool {
		self.parts.is_empty() || self.parts.iter().all(Part::is_empty)
	}
}

impl Part {
	pub fn is_empty(&self) -> bool {
		match self {
			Part::Text(s) => s.is_empty(),
			Part::InlineData(blob) => blob.data.is_empty(),
		}
	}
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Role {
	Model,
	User,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Part {
	Text(String),
	InlineData(GenerativeContentBlob),
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SetupConfig {
	// pub tools: Option<Vec<Tool>>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LiveGenerationConfig {
	pub response_modalities: Vec<Modality>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub speech_config: Option<SpeechConfig>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Modality {
	Text,
	Audio,
	Image,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SpeechConfig {
	pub voice_config: Option<VoiceConfig>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VoiceConfig {
	pub prebuilt_voice_config: Option<PrebuiltVoiceConfig>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PrebuiltVoiceConfig {
	pub voice_name: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum OutgoingMessage {
	RealtimeInput {
		media_chunks: Vec<GenerativeContentBlob>,
	},
	Setup {
		model: String,
		#[serde(skip_serializing_if = "Content::is_empty")]
		system_instruction: Content,
		generation_config: Option<LiveGenerationConfig>,
	},
	ClientContent {
		turns: Vec<Content>,
		turn_complete: bool,
	},
}

#[derive(Deserialize, Serialize, Debug)]
pub struct GenerativeContentBlob {
	pub data: String,
	pub mime_type: String,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ServerContentMessage {
	pub server_content: ServerContent,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum ServerContent {
	ModelTurn(Content),
	TurnComplete(bool),
	Interrupted(bool),
}

pub struct GeminiAudioStream {
	pub(crate) data: Vec<u8>,
}

impl GeminiAudioStream {
	pub fn new(content: Content) -> Self {
		let mut data = Vec::new();

		for part in content.parts {
			match part {
				Part::InlineData(blob) => {
					data.extend_from_slice(blob.data.as_bytes());
				}
				Part::Text(_) => {}
			}
		}

		Self { data }
	}
}

impl Compose for GeminiAudioStream {
	fn create(
		&mut self,
	) -> Result<
		AudioStream<Box<dyn MediaSource>>,
		songbird::input::AudioStreamError,
	> {
		let cursor = Cursor::new(self.data.clone());
		Ok(AudioStream {
			input: Box::new(cursor),
			hint: None,
		})
	}

	fn should_create_async(&self) -> bool {
		false
	}

	fn create_async<'a, 'async_trait>(
		&'a mut self,
	) -> ::core::pin::Pin<
		Box<
			dyn ::core::future::Future<
					Output = Result<
						AudioStream<Box<dyn MediaSource>>,
						AudioStreamError,
					>,
				> + ::core::marker::Send
				+ 'async_trait,
		>,
	>
	where
		'a: 'async_trait,
		Self: 'async_trait,
	{
		Box::pin(async { self.create() })
	}
}
