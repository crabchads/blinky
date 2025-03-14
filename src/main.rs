use std::future::Future;
use std::sync::Arc;
use std::{env, mem};

use commands::JoinCommand;
use tokio::sync::Mutex;
use tracing::Instrument;
use twilight_cache_inmemory::{DefaultInMemoryCache, ResourceType};
use twilight_gateway::{
	Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _,
};
use twilight_http::Client;
use twilight_interactions::command::CreateCommand as _;
use twilight_model::application::interaction::application_command::CommandData;
use twilight_model::application::interaction::Interaction;
use twilight_model::id::marker::ApplicationMarker;
use twilight_model::oauth::Application;
use twilight_model::{application::interaction::InteractionData, id::Id};
use twilight_standby::Standby;

mod backend;
mod commands;
mod discord_voice;

type Error = Box<dyn std::error::Error + Send + Sync>;

struct State {
	client: Client,
	voice_session_id: Option<String>,
	voice_token: Option<String>,
	voice_client: Option<Arc<Mutex<discord_voice::DiscordVoiceClient>>>,
	backend: Arc<Mutex<backend::Backend>>,
	shard: Shard,
	standby: Standby,
	cache: DefaultInMemoryCache,
	application: Application,
}

#[tokio::main]
#[dotenvy::load]
async fn main() -> Result<(), Error> {
	tracing_subscriber::fmt().compact().try_init()?;

	let token =
		env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

	let state = Arc::new(Mutex::new(State::connect(token).await?));
	state.lock().await.setup().await?;

	Ok(())
}

impl State {
	pub async fn connect(token: String) -> Result<Self, Error> {
		let client = Client::new(token.clone());

		let application =
			client.current_user_application().await?.model().await?;

		Ok(Self {
			backend: Arc::new(Mutex::new(backend::Backend::connect().await)),
			client,
			voice_session_id: None,
			voice_token: None,
			voice_client: None,
			application,
			shard: Shard::new(ShardId::ONE, token, Intents::GUILD_VOICE_STATES),
			cache: DefaultInMemoryCache::builder()
				.resource_types(ResourceType::MESSAGE)
				.build(),
			standby: Standby::new(),
		})
	}

	pub async fn setup(&mut self) -> Result<(), Error> {
		let interaction_client = self.client.interaction(self.application.id);
		let commands = [JoinCommand::create_command().into()];

		tracing::info!(
			"logged as {} with ID {}",
			self.application.name,
			self.application.id
		);

		if let Err(error) = interaction_client
			.set_guild_commands("301526452770439168".parse()?, &commands)
			.await
		{
			tracing::error!(?error, "failed to register commands");
		}

		while let Some(item) =
			self.shard.next_event(EventTypeFlags::all()).await
		{
			let Ok(event) = item else {
				tracing::warn!(source = ?item.unwrap_err(), "error receiving event");

				continue;
			};

			self.cache.update(&event);

			tracing::info!(?event, "received event");

			let span = tracing::info_span!("event", ?event);

			self.handle_event(event).instrument(span).await?;
		}

		Ok(())
	}

	fn handle_event(
		&mut self,
		event: Event,
	) -> impl Future<Output = Result<(), Error>> {
		self.standby.process(&event);

		async move {
			match event {
				Event::InteractionCreate(mut interaction) => {
					let data = match mem::take(&mut interaction.data) {
						Some(InteractionData::ApplicationCommand(data)) => {
							*data
						}
						_ => {
							tracing::warn!("ignoring non-command interaction");
							return Ok(());
						}
					};

					if let Err(error) = self
						.handle_command(
							interaction.0,
							data,
							self.application.id,
						)
						.await
					{
						tracing::error!(?error, "error while handling command");
					}
				}
				Event::VoiceStateUpdate(data) => {
					self.voice_session_id = Some(data.session_id.clone());
				}
				Event::VoiceServerUpdate(data) => {
					self.voice_token = Some(data.token.clone());

					let user_id = self
						.client
						.current_user()
						.await?
						.model()
						.await?
						.id
						.to_string();

					// get the sequence number of last numbered message received from the gateway

					self.voice_client = Some(Arc::new(Mutex::new(
						discord_voice::DiscordVoiceClient::connect(
							data.endpoint.as_ref().unwrap().to_string(),
							self.voice_session_id.as_ref().unwrap().to_string(),
							self.voice_token.as_ref().unwrap().to_string(),
							data.guild_id.get(),
							user_id.parse().unwrap(),
						)
						.await?,
					)));

					// start the heartbeat loop
					tokio::spawn({
						let voice_client =
							Arc::clone(self.voice_client.as_ref().unwrap());

						async move {
							loop {
								let sequence =
									self.shard.session().unwrap().sequence();

								tokio::time::sleep(
									std::time::Duration::from_millis(
										voice_client
											.lock()
											.await
											.heartbeat_interval,
									),
								)
								.await;
								voice_client
									.lock()
									.await
									.heartbeat(sequence)
									.await;
							}
						}
					});
				}
				_ => {}
			}

			Ok(())
		}
	}

	async fn handle_command(
		&mut self,
		interaction: Interaction,
		data: CommandData,
		application_id: Id<ApplicationMarker>,
	) -> Result<(), Error> {
		match &*data.name {
			"join" => {
				JoinCommand::handle(
					interaction,
					data,
					&self.client,
					&self.shard.sender(),
					application_id,
				)
				.await
			}
			name => Err(format!("unknown command: {}", name).into()),
		}
	}
}
