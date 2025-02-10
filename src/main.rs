use std::env;
use std::sync::Arc;

use crate::gemini::GeminiLiveClient;
use async_trait::async_trait;
use gemini::types::{GeminiAudioStream, ServerContent};
use poise::serenity_prelude as serenity;
use serenity::GatewayIntents;
use songbird::driver::DecodeMode;
use songbird::events::EventHandler;
use songbird::input::{Compose, Input, LiveInput};
use songbird::tracks::Track;
use songbird::{
	Config, CoreEvent, Event, EventContext, SerenityInit, TrackEvent,
};
use tokio::sync::Mutex;

mod gemini;

struct Data {
	gemini: Arc<Mutex<GeminiLiveClient>>,
}

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

#[poise::command(prefix_command, slash_command)]
async fn join(context: Context<'_>) -> Result<(), Error> {
	let (guild_id, channel_id) = {
		let guild = context.guild().unwrap();
		let channel_id = guild
			.voice_states
			.get(&context.author().id)
			.and_then(|voice_state| voice_state.channel_id);

		(guild.id, channel_id)
	};

	let connect_to = match channel_id {
		Some(channel) => channel,
		None => {
			poise::say_reply(context, "You're not in a voice channel!").await?;

			return Ok(());
		}
	};

	let manager = songbird::get(context.serenity_context())
		.await
		.expect("Songbird Voice client placed in at initialisation.")
		.clone();

	if let Ok(handler_lock) = manager.join(guild_id, connect_to).await {
		// Attach an event handler to see notifications of all track errors.
		let mut handler = handler_lock.lock().await;
		handler.add_global_event(
			TrackEvent::Error.into(),
			VoiceNotifier {
				handler: Arc::clone(&handler_lock),
				gemini: Arc::clone(&context.data().gemini),
			},
		);
		handler.add_global_event(
			CoreEvent::VoiceTick.into(),
			VoiceNotifier {
				handler: Arc::clone(&handler_lock),
				gemini: Arc::clone(&context.data().gemini),
			},
		);
	}

	context.reply("Joined your channel!").await?;

	Ok(())
}

struct VoiceNotifier {
	gemini: Arc<Mutex<GeminiLiveClient>>,
	handler: Arc<Mutex<songbird::Call>>,
}

#[async_trait]
impl EventHandler for VoiceNotifier {
	async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
		if let EventContext::Track(track_list) = ctx {
			for (state, handle) in *track_list {
				println!(
					"Track {:?} encountered an error: {:?}",
					handle.uuid(),
					state.playing
				);
			}
		} else if let EventContext::VoiceTick(tick) = ctx {
			// println!("Voice tick: {:?}", tick);

			let mut gemini = self.gemini.lock().await;

			let mut audio_data = Vec::new();
			for voice_data in tick.speaking.values() {
				if let Some(decoded_voice) = &voice_data.decoded_voice {
					audio_data.extend_from_slice(
						&decoded_voice
							.iter()
							.flat_map(|x| x.to_le_bytes())
							.collect::<Vec<u8>>(),
					);
				}
			}

			if !audio_data.is_empty() {
				match gemini
					.send_realtime_audio(audio_data, "audio/pcm;rate=48000")
					.await
				{
					Err(err) => {
						println!("Failed to send audio: {:?}", err);
					}
					Ok(response) => {
						if let ServerContent::ModelTurn(turn) = response {
							let mut stream = GeminiAudioStream::new(turn);

							let input = Input::Live(
								LiveInput::Raw(stream.create().unwrap()),
								None,
							);

							self.handler.lock().await.play(Track::new(input));
						}
					}
				}
			}
		}

		None
	}
}

#[tokio::main]
async fn main() {
	tracing_subscriber::fmt::init();

	let token =
		env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

	let mut intents = GatewayIntents::non_privileged();
	intents |= GatewayIntents::GUILD_VOICE_STATES;

	let framework = poise::Framework::builder()
		.options(poise::FrameworkOptions {
			commands: vec![join()],
			..Default::default()
		})
		.setup(|ctx, _ready, framework| {
			Box::pin(async move {
				let commands = poise::builtins::create_application_commands(
					&framework.options().commands,
				);

				serenity::GuildId::new(301526452770439168)
					.set_commands(ctx, commands)
					.await?;
				let data = Data {
					gemini: Arc::new(Mutex::new(
						GeminiLiveClient::connect().await?,
					)),
				};

				data.gemini.lock().await.setup().await?;

				Ok(data)
			})
		})
		.build();

	let songbird_config = Config::default().decode_mode(DecodeMode::Decode);

	let client = serenity::ClientBuilder::new(token, intents)
		.framework(framework)
		.register_songbird_from_config(songbird_config)
		.await;
	client.unwrap().start().await.unwrap();
}
