use std::sync::mpsc;

use super::api::ApiMessage;

//Typedefs for mpsc channels
pub type Tx = mpsc::Sender<ApiMessage>;
pub type Rx = mpsc::Receiver<ApiMessage>;

pub type MTx = mpsc::Sender<String>;
pub type MRx = mpsc::Receiver<String>;

///Spawns external api and waits for close signal
pub fn spawn_api(token: String, user_tx: Tx, music_rx: MRx)
{
    let (close_tx, close_rx) = mpsc::channel::<ApiMessage>();

    // Configure the client with your Discord bot token in the environment.
    let mut client = Client::new(&token, Handler).expect("Err creating client");

    // Obtain a lock to the data owned by the client, and insert the client's
    // voice manager into it. This allows the voice manager to be accessible by
    // event handlers and framework commands.
    {
        let mut data = client.data.write();
        data.insert::<VoiceManager>(Arc::clone(&client.voice_manager));
        data.insert::<ApiSender>(Arc::new(Mutex::new(user_tx)));
        data.insert::<MusicReceiver>(Arc::new(Mutex::new(music_rx)));
        data.insert::<CloseSender>(Arc::new(Mutex::new(close_tx)));
    }

    client.with_framework(StandardFramework::new()
        .configure(|c| c
            .prefix("~"))
        .group(&GENERAL_GROUP));
    let _ = client.start().map_err(|why| println!("Client ended: {:?}", why));
    close_rx.recv().unwrap();
}

//Api code at the bottom to allow for easy removal
use std::sync::Arc;

use serenity::{
    client::{bridge::voice::ClientVoiceManager, Client, Context, EventHandler},
    framework::{
        StandardFramework,
        standard::{
            Args, CommandResult,
            macros::{command, group},
        },
    },
    model::{channel::Message, gateway::Ready, id::ChannelId},
    prelude::*,
    voice::{AudioReceiver, ytdl},
    voice::Handler as VoiceHandler,
    Result as SerenityResult,
};

//Definitions for data to send to Serenity api
struct VoiceManager;

impl TypeMapKey for VoiceManager {
    type Value = Arc<Mutex<ClientVoiceManager>>;
}

struct ApiSender;
type ApiSenderType = Arc<Mutex<Tx>>;

impl TypeMapKey for ApiSender {
    type Value = Arc<Mutex<Tx>>;
}

struct MusicReceiver;
type MusicReceiverType = Arc<Mutex<MRx>>;

impl TypeMapKey for  MusicReceiver{
    type Value = Arc<Mutex<MRx>>;
}

struct CloseSender;

impl TypeMapKey for CloseSender {
    type Value = Arc<Mutex<Tx>>;
}


//Event Listeners
struct Handler;

impl EventHandler for Handler {
    fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

struct Receiver
{
    handler: VoiceHandler,
    api_tx: ApiSenderType,
    music_rx: MusicReceiverType,
}

impl Receiver
{
    fn new(handler: VoiceHandler, 
           api_tx: ApiSenderType, music_rx: MusicReceiverType) -> Self
    {
        Self{handler: handler, api_tx: api_tx, music_rx: music_rx}
    }
}

impl AudioReceiver for Receiver {
    fn client_connect(&mut self, _ssrc: u32, _user_id: u64) {
        let api = self.api_tx.lock();
        match api.send(ApiMessage::User(_user_id.to_string()))
        {
            Ok(_msg) => (),
            Err(_msg) => 
            {
                println!("{}", format!("Unable to send {} through api", 
                                       _user_id.to_string()))
            }
        }
        let response = self.music_rx.lock();
        let music_url = response.recv().unwrap();
        let music = ytdl(&music_url).unwrap();
        self.handler.play(music);
    }
}


#[group]
#[commands(join, leave, logout)]
struct General;

#[command]
fn join(ctx: &mut Context, msg: &Message, mut args: Args) -> CommandResult {
    let connect_to = match args.single::<u64>() {
        Ok(id) => ChannelId(id),
        Err(_) => {
            check_msg(msg.reply(&ctx, "Requires a valid voice channel ID be given"));

            return Ok(());
        },
    };

    let guild_id = match ctx.cache.read().guild_channel(msg.channel_id) {
        Some(channel) => channel.read().guild_id,
        None => {
            check_msg(msg.channel_id.say(&ctx.http, "Groups and DMs not supported"));

            return Ok(());
        },
    };

    let manager_lock = ctx.data.read().get::<VoiceManager>().cloned()
        .expect("Expected VoiceManager in ShareMap.");
    let mut manager = manager_lock.lock();
    let api_tx = ctx.data.read().get::<ApiSender>().cloned().unwrap();
    let music_rx = ctx.data.read().get::<MusicReceiver>().cloned().unwrap();

    if let Some(handler) = manager.join(guild_id, connect_to) {
        handler.listen(Some(Box::new(Receiver::new(handler.clone(), api_tx, music_rx))));
    } else {
        check_msg(msg.channel_id.say(&ctx.http, "Error joining the channel"));
    }

    Ok(())
}

#[command]
fn leave(ctx: &mut Context, msg: &Message) -> CommandResult {
    let guild_id = match ctx.cache.read().guild_channel(msg.channel_id) {
        Some(channel) => channel.read().guild_id,
        None => {
            check_msg(msg.channel_id.say(&ctx.http, "Groups and DMs not supported"));

            return Ok(());
        },
    };

    let manager_lock = ctx.data.read().get::<VoiceManager>().cloned()
        .expect("Expected VoiceManager in ShareMap.");
    let mut manager = manager_lock.lock();
    let has_handler = manager.get(guild_id).is_some();

    if has_handler {
        manager.remove(guild_id);
    } else {
        check_msg(msg.reply(&ctx, "Not in a voice channel"));
    }
    Ok(())
}

#[command]
fn logout(ctx: &mut Context, _msg: &Message) -> CommandResult {
    let close_tx = ctx.data.read().get::<CloseSender>().cloned().unwrap();
    let close_tx = close_tx.lock();
    match close_tx.send(ApiMessage::Logout())
    {
        Ok(_msg) => (),
        Err(_msg) => 
        {
            println!("{}", "Unable to send serenity close signal."); 
        }
    }

    let api_tx = ctx.data.read().get::<ApiSender>().cloned().unwrap();
    let api_tx = api_tx.lock();
    match api_tx.send(ApiMessage::Logout())
    {
        Ok(_msg) => (),
        Err(_msg) => 
        {
            println!("{}","Unable to send DynamicBot logout signal");
        }
    }

    Ok(())
}


//Helper functions

/// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}
