use dotenv::dotenv;
use std::{env, error::Error, sync::Arc};
use tokio::sync::RwLock;
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{Event, Intents, Shard, ShardId};
use twilight_http::Client as HttpClient;
use twilight_model::{
    application::{
        command::CommandType,
        interaction::{application_command::CommandOptionValue, InteractionData},
    },
    guild::Permissions,
    http::interaction::{InteractionResponse, InteractionResponseType},
    id::{
        marker::{ApplicationMarker, ChannelMarker, GuildMarker, UserMarker},
        Id,
    },
};
use twilight_util::builder::{
    command::{CommandBuilder, UserBuilder},
    embed::{EmbedAuthorBuilder, EmbedBuilder, ImageSource},
    InteractionResponseDataBuilder,
};

struct Client {
    pub http: Arc<HttpClient>,
    pub cache: Arc<InMemoryCache>,
    pub shard: Arc<RwLock<Shard>>,
    pub application_id: Id<ApplicationMarker>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    dotenv().ok();
    env_logger::init();
    let token = env::var("DISCORD_TOKEN")?;

    let intents = Intents::GUILD_MESSAGES
        | Intents::DIRECT_MESSAGES
        | Intents::MESSAGE_CONTENT
        | Intents::GUILDS;

    let shard_lock = Arc::new(RwLock::new(Shard::new(ShardId::ONE, token.clone(), intents)));

    let http = Arc::new(HttpClient::new(token));

    let cache = Arc::new(
        InMemoryCache::builder()
            .resource_types(ResourceType::MESSAGE | ResourceType::CHANNEL | ResourceType::GUILD)
            .build(),
    );
    let application_id = {
        let response = http.current_user_application().await?;
        response.model().await?.id
    };
    let client = Arc::new(Client {
        http: Arc::clone(&http),
        cache: Arc::clone(&cache),
        shard: Arc::clone(&shard_mutex),
        application_id,
    });
    create_application_commands(Arc::clone(&client)).await?;

    loop {
        let shard = shard_lock.write().await;
        let event = match shard.next_event().await {
            Ok(event) => event,
            Err(source) => {
                tracing::warn!(?source, "error receiving event");

                if source.is_fatal() {
                    break;
                }

                continue;
            }
        };
        cache.update(&event);

        tokio::spawn(handle_event(event, Arc::clone(&client)));
    }

    Ok(())
}

async fn create_application_commands(
    client: Arc<Client>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let interaction = client.http.interaction(client.application_id);
    let commands = [
        CommandBuilder::new("ping", "bot ping", CommandType::ChatInput).build(),
        CommandBuilder::new("close", "close some ticket", CommandType::ChatInput).build(),
        CommandBuilder::new("kick", "Kick some user", CommandType::ChatInput)
            .option(UserBuilder::new("user", "user to kick").required(true))
            .build(),
        CommandBuilder::new("ban", "Ban some user", CommandType::ChatInput)
            .option(UserBuilder::new("user", "user to ban user").required(true))
            .build(),
    ];
    interaction.set_global_commands(&commands).await?;
    Ok(())
}

async fn handle_event(
    event: Event,
    client: Arc<Client>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match event {
        Event::Ready(_) => {
            println!("Shard is ready");
        }
        Event::MessageCreate(msg) => {
            if msg.author.bot {
                return Ok(());
            }
            if msg.guild_id.is_none() {
                // DM to moderator
                let category_id: u64 = env::var("CATEGORY_ID")?.parse()?;
                let parent_id: Id<ChannelMarker> = Id::new(category_id);
                let channels = client.cache.iter().channels().filter(|channel| {
                    channel.topic == Some(msg.author.id.to_string())
                        && channel.parent_id == Some(parent_id)
                });
                let guild_id: u64 = env::var("GUILD_ID")?.parse()?;
                let guild_id: Id<GuildMarker> = Id::new(guild_id);
                let channel_id: Id<ChannelMarker> = match channels.last() {
                    Some(channel) => channel.id,
                    None => {
                        let channel = client
                            .http
                            .create_guild_channel(guild_id, &msg.author.name.clone())?
                            .parent_id(parent_id)
                            .topic(&msg.author.id.to_string())?
                            .await?
                            .model()
                            .await?;
                        channel.id
                    }
                };
                let mut avatar_url = String::new();
                if let Some(avatar) = msg.author.avatar {
                    avatar_url = format!(
                        "https://cdn.discordapp.com/avatars/{}/{}.png",
                        msg.author.id, avatar
                    )
                }
                let image_source = ImageSource::url(avatar_url)?;
                let mut embed = EmbedBuilder::new()
                    .description(&msg.content)
                    .author(EmbedAuthorBuilder::new(msg.author.name.clone()).icon_url(image_source))
                    .timestamp(msg.timestamp);
                if !msg.attachments.is_empty() {
                    embed = embed.image(ImageSource::url(msg.attachments[0].url.clone())?);
                }
                let embed = embed.build();
                client
                    .http
                    .create_message(channel_id)
                    .embeds(&[embed])?
                    .await?;
            } else {
                // Moderator to DM
                let parent_id = env::var("CATEGORY_ID")?.parse()?;
                let parent_id: Id<ChannelMarker> = Id::new(parent_id);
                let channel = client.cache.channel(msg.channel_id).unwrap();
                if let Some(base_parent_id) = channel.parent_id {
                    if parent_id == base_parent_id {
                        let user_id: u64 = channel.topic.clone().unwrap().parse()?;
                        let user_id: Id<UserMarker> = Id::new(user_id);
                        let channel = client
                            .http
                            .create_private_channel(user_id)
                            .await?
                            .model()
                            .await?;
                        let guild = client.cache.guild(msg.guild_id.unwrap()).unwrap();
                        let icon_url = format!(
                            "https://cdn.discordapp.com/icons/{}/{}.png",
                            guild.id(),
                            guild.icon().unwrap()
                        );
                        let mut embed = EmbedBuilder::new()
                            .description(&msg.content)
                            .author(
                                EmbedAuthorBuilder::new("運営(Moderator)")
                                    .icon_url(ImageSource::url(icon_url)?),
                            )
                            .timestamp(msg.timestamp);
                        if !msg.attachments.is_empty() {
                            embed = embed.image(ImageSource::url(msg.attachments[0].url.clone())?);
                        }
                        let embed = embed.build();
                        client
                            .http
                            .create_message(channel.id)
                            .embeds(&[embed])?
                            .await?;
                    }
                }
            }
        }
        Event::InteractionCreate(interaction) => {
            let interaction_http = client.http.interaction(client.application_id);
            if let Some(InteractionData::ApplicationCommand(command)) = &interaction.data {
                if command.name == "ping" {
                    let response = InteractionResponse {
                        kind: InteractionResponseType::DeferredChannelMessageWithSource,
                        data: None,
                    };
                    interaction_http
                        .create_response(interaction.id, &interaction.token, &response)
                        .await?;
                    println!("defer");
                    let shard = client.shard.read().await;
                    let latency = shard.latency();
                    let average = latency.average().unwrap();
                    interaction_http
                        .create_followup(&interaction.token)
                        .content(&format!("Pong!\n{}", average.as_micros()).to_string())?
                        .await?;
                } else if command.name == "close" {
                    let parent_id: u64 = env::var("CATEGORY_ID")?.parse()?;
                    let parent_id: Id<ChannelMarker> = Id::new(parent_id);
                    if interaction.channel.clone().unwrap().parent_id != Some(parent_id) {
                        let data = InteractionResponseDataBuilder::new()
                            .content(
                                "このコマンドはチケットチャンネルでのみ使用できます。".to_string(),
                            )
                            .build();
                        let response = InteractionResponse {
                            kind: InteractionResponseType::ChannelMessageWithSource,
                            data: Some(data),
                        };
                        interaction_http
                            .create_response(interaction.id, &interaction.token, &response)
                            .await?;
                        return Ok(());
                    }
                    let channel = interaction.channel.clone().unwrap();
                    let userid = channel.topic.unwrap().parse::<u64>().unwrap();
                    let userid: Id<UserMarker> = Id::new(userid);
                    let channel = client
                        .http
                        .create_private_channel(userid)
                        .await?
                        .model()
                        .await?;
                    let embed = EmbedBuilder::new()
                            .title("問い合わせ")
                            .description(
                                "問い合わせを運営が終了しました\nまだ問題解決していない場合はお手数ですが、再度お問い合わせをお願いします。"
                            )
                            .color(0xf50505)
                            .build();
                    client
                        .http
                        .create_message(channel.id)
                        .embeds(&[embed])?
                        .await?;
                    let data = InteractionResponseDataBuilder::new()
                        .content("お問い合わせを閉じました。".to_string())
                        .build();
                    let response = InteractionResponse {
                        kind: InteractionResponseType::ChannelMessageWithSource,
                        data: Some(data),
                    };
                    interaction_http
                        .create_response(interaction.id, &interaction.token, &response)
                        .await?;
                    client
                        .http
                        .delete_channel(interaction.channel.clone().unwrap().id)
                        .await?;
                } else if command.name == "kick" {
                    if interaction
                        .member
                        .clone()
                        .unwrap()
                        .permissions
                        .unwrap()
                        .contains(Permissions::KICK_MEMBERS)
                    {
                        println!("Checked permissions");
                        if let CommandOptionValue::User(userid) =
                            command.options.get(0).unwrap().value
                        {
                            let response = InteractionResponse {
                                kind: InteractionResponseType::DeferredChannelMessageWithSource,
                                data: None,
                            };
                            interaction_http
                                .create_response(interaction.id, &interaction.token, &response)
                                .await?;
                            println!("defer");
                            client
                                .http
                                .remove_guild_member(interaction.guild_id.unwrap(), userid)
                                .await?;
                            println!("removed");
                            interaction_http
                                .create_followup(&interaction.token)
                                .content("<:ok_handbutflipped:779364331350523909>")?
                                .await?;
                        }
                    } else {
                        let data = InteractionResponseDataBuilder::new()
                            .content("このコマンドは運営のみ使用できます。".to_string())
                            .build();
                        let response = InteractionResponse {
                            kind: InteractionResponseType::ChannelMessageWithSource,
                            data: Some(data),
                        };
                        interaction_http
                            .create_response(interaction.id, &interaction.token, &response)
                            .await?;
                    }
                }
            }
        }
        _ => {}
    }

    Ok(())
}
