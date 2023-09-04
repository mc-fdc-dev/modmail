use dotenv::dotenv;
use std::{env, error::Error, sync::Arc};
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{Event, Intents, Shard, ShardId};
use twilight_http::Client as HttpClient;
use twilight_model::id::{marker::{ChannelMarker, GuildMarker}, Id};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    dotenv().ok();
    let token = env::var("DISCORD_TOKEN")?;

    let intents = Intents::GUILD_MESSAGES | Intents::DIRECT_MESSAGES | Intents::MESSAGE_CONTENT;

    let mut shard = Shard::new(ShardId::ONE, token.clone(), intents);

    let http = Arc::new(HttpClient::new(token));

    let cache = Arc::new(
        InMemoryCache::builder()
            .resource_types(ResourceType::MESSAGE | ResourceType::GUILD)
            .build(),
    );

    loop {
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

        tokio::spawn(handle_event(event, Arc::clone(&http), Arc::clone(&cache)));
    }

    Ok(())
}

async fn handle_event(
    event: Event,
    http: Arc<HttpClient>,
    cache: Arc<InMemoryCache>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match event {
        Event::MessageCreate(msg) => {
            if msg.author.bot {
                return Ok(());
            }
            if msg.guild_id.is_some() {
                return Ok(());
            }
            let category_id: u64 = env::var("CATEGORY_ID")?.parse()?;
            let parent_id: Id<ChannelMarker> = Id::new(category_id);
            let channels = cache.iter().channels().filter(|channel| {
                channel.topic == Some(msg.author.id.to_string())
                    && channel.parent_id == Some(parent_id)
            });
            let guild_id: u64 = env::var("GUILD_ID")?.parse()?;
            let guild_id: Id<GuildMarker> = Id::new(guild_id);
            let channel_id: Id<ChannelMarker> = match channels.last() {
                Some(channel) => {
                    channel.id
                },
                None => {
                    let channel = http
                        .create_guild_channel(guild_id, &msg.author.name.clone())?
                        .parent_id(parent_id)
                        .topic(&msg.author.id.to_string())?
                        .await?
                        .model()
                        .await?;
                    println!("Create channel");
                    channel.id
                }
            };
            println!("channel_id");
            http.create_message(channel_id)
                .content(&msg.content)?
                .await?;
        }
        Event::Ready(_) => {
            println!("Shard is ready");
        }
        _ => {}
    }

    Ok(())
}
