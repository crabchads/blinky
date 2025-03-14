use twilight_gateway::MessageSender;
use twilight_http::{client::InteractionClient, Client};
use twilight_interactions::command::{
	CommandModel, CreateCommand, DescLocalizations,
};
use twilight_model::{
	application::interaction::{application_command::CommandData, Interaction},
	channel::message::Embed,
	gateway::payload::outgoing::UpdateVoiceState,
	http::interaction::{InteractionResponse, InteractionResponseType},
	id::{
		marker::{ApplicationMarker, ChannelMarker},
		Id,
	},
};
use twilight_util::builder::InteractionResponseDataBuilder;

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "join", desc_localizations = "join_desc")]
pub struct JoinCommand {
	#[command(desc_localizations = "channel_id_desc")]
	channel_id: String,
}

fn join_desc() -> DescLocalizations {
	DescLocalizations::new(
		"Join a voice channel",
		[("en-US", "Join a voice channel")],
	)
}

fn channel_id_desc() -> DescLocalizations {
	DescLocalizations::new(
		"ID of the voice channel to join",
		[("en-US", "ID of the voice channel to join")],
	)
}

impl JoinCommand {
	#[tracing::instrument(skip(
		interaction,
		data,
		client,
		sender,
		application_id,
	))]
	pub async fn handle(
		interaction: Interaction,
		data: CommandData,
		client: &Client,
		sender: &MessageSender,
		application_id: Id<ApplicationMarker>,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		let command = JoinCommand::from_interaction(data.into())
			.map_err(|e| format!("Failed to parse command: {}", e))?;

		let guild_id = interaction.guild_id.expect("Expected a guild ID");

		let channel_id = command
			.channel_id
			.parse::<Id<ChannelMarker>>()
			.expect("Expected a channel ID");

		let guild = client.guild(guild_id).await?.model().await;
		let channel = client.channel(channel_id).await?.model().await;

		// join the voice channel
		match (guild, channel) {
			(Ok(_guild), Ok(_channel)) => {
				sender.command(&UpdateVoiceState::new(
					guild_id,
					Some(channel_id),
					false,
					false,
				))?;

				let response = InteractionResponse {
					data: Some(
						InteractionResponseDataBuilder::new()
							.content("Joined voice channel")
							.build(),
					),
					kind: InteractionResponseType::ChannelMessageWithSource,
				};

				client
					.interaction(application_id)
					.create_response(
						interaction.id,
						&interaction.token,
						&response,
					)
					.await?;

				Ok(())
			}
			_ => {
				let response = InteractionResponse {
					data: Some(
						InteractionResponseDataBuilder::new()
							.content("Failed to join voice channel")
							.build(),
					),
					kind: InteractionResponseType::ChannelMessageWithSource,
				};

				client
					.interaction(application_id)
					.create_response(
						interaction.id,
						&interaction.token,
						&response,
					)
					.await?;

				Ok(())
			}
		}
	}
}
