#![allow(clippy::useless_let_if_seq)]
#[macro_use]
extern crate diesel;

pub mod models;
pub mod schema;

use self::models::*;
use diesel::{
  mysql::MysqlConnection,
  prelude::*,
  r2d2::{
    ConnectionManager,
    Pool
  },
  result::{
    Error as DieselError,
    DatabaseErrorKind
  },
  RunQueryDsl,
};
use dotenv::dotenv;
use retry::{delay::Fixed, retry, OperationResult};
use serde_json::json;
use lazy_static::lazy_static;
use serenity::{
  client::Client,
  framework::standard::{
    macros::{command, group},
    Args, CommandResult, StandardFramework,
  },
  model::{channel::Message, guild::Member, id::GuildId, user::User},
  prelude::{Context, EventHandler},
};
use std::{env, fs::File, vec};
use url::Url;

group!({
  name: "general",
  options: {},
  commands: [
    mclink,
    unlink
  ],
});

const MOJANG_GET_HISTORY: &str = "https://api.mojang.com/user/profiles/";
const MOJANG_GET_UUID: &str = "https://api.mojang.com/profiles/minecraft";

struct Handler;

impl EventHandler for Handler {
  fn guild_member_removal(&self, _ctx: Context, guild: GuildId, user: User, _member_data_if_available: Option<Member>) {
    let discord_vals: DiscordConfig = get_config().discord;

    if &discord_vals.guild_id == guild.as_u64() {
      println!("{} is leaving Mooncord", user.name);

      rem_account(*user.id.as_u64());
    }
  }
}

lazy_static! {
  static ref POOL: Pool<ConnectionManager<MysqlConnection>> = establish_connection();
}

fn issue_cmd(conn: &mut rcon::Connection, cmd: &str) -> OperationResult<String, String> {
  match conn.cmd(cmd) {
    Ok(val) => {
      println!("{}", val);

      OperationResult::Ok(val)
    }
    Err(why) => {
      println!("RCON Failure: {:?}", why);

      OperationResult::Retry(format!("{:?}", why))
    }
  }
}

fn establish_connection() -> Pool<ConnectionManager<MysqlConnection>> {
  dotenv().ok();

  let db_url = env::var("DATABASE_URL").expect("DATABASE_URL env var must be set");
  let manager = ConnectionManager::<MysqlConnection>::new(db_url);
  
  Pool::builder()
    .build(manager)
    .expect("Failed to create pool")
}

fn get_config() -> ConfigSchema {
  let f = File::open("./config.yaml").unwrap();

  serde_yaml::from_reader(&f).unwrap()
}

fn main() {
  let discord_vals: DiscordConfig = get_config().discord;

  // Bot login
  let mut client: Client =
    Client::new(&discord_vals.token, Handler).expect("Error creating client");

  client.with_framework(
    StandardFramework::new()
      .configure(|c| c.prefix("!"))
      .group(&GENERAL_GROUP),
  );

  // Start listening for events, single shard. Shouldn't need more than one shard
  if let Err(why) = client.start() {
    println!("An error occurred while running the client: {:?}", why);
  }
}

fn add_accounts(discordid: u64, mc_user: &MinecraftUser) -> QueryResult<usize> {
  use self::schema::minecrafters;

  let connection = POOL.get().unwrap();

  let mcid = &mc_user.id;
  let mcname = &mc_user.name;

  let new_user = NewMinecraftUser {
    discord_id: discordid,
    minecraft_uuid: mcid.to_string(),
    minecraft_name: mcname.to_string(),
  };

  let res = diesel::insert_into(minecrafters::table)
    .values(&new_user)
    .execute(&connection);

  res
}

fn whitelist_account(mc_user: &MinecraftUser, towhitelist: bool) -> u8 {
  let mc_servers: Vec<MinecraftServerIdentity> = get_config().minecraft.servers;

  for server in &mc_servers {
    let act: String = format!("{}", if towhitelist { "add" } else { "remove" });
    let address: String = format!("{}:{}", &server.ip, &server.port);
    let cmd: String = format!("whitelist {} {}", act, mc_user.name);

    let res = retry(Fixed::from_millis(2000).take(10), || {
      match rcon::Connection::connect(&address, &server.pass) {
        Ok(mut val) => issue_cmd(&mut val, &cmd),
        Err(why) => {
          println!("Error connecting to server: {:?}", why);

          OperationResult::Retry(format!("{:?}", why))
        }
      }
    });

    let ok = &res.is_ok();

    if *ok && res.unwrap() == "That player does not exist" {
      return 2
    }

    if !*ok {
      return 1
    }
  }

  0
}

fn sel_mc_account(_discord_id: u64) -> Option<MinecraftUser> {
  use self::schema::minecrafters::dsl::*;

  let connection = POOL.get().unwrap();

  let res = minecrafters.filter(discord_id.eq(_discord_id))
    .load::<FullMCUser>(&connection)
    .expect("Error loading minecraft user");

  if res.len() < 1 {
    println!("[WARN] NO PLAYER FOUND BY DISCORD ID");
    return None
  }

  let mcid = &res[0].minecraft_uuid;
  let mcname = &res[0].minecraft_name;

  let mc_user = MinecraftUser {
    id: mcid.to_string(),
    name: mcname.to_string(),
  };

  Some(mc_user)
}

fn rem_account(discordid: u64) -> bool {
  use self::schema::minecrafters::dsl::*;

  // Retrieve MC account for whitelist removal
  let user: Option<MinecraftUser> = sel_mc_account(discordid);

  if user.is_none() {
    // User was never whitelisted or manually removed
    return false;
  }

  // Overwrite with val
  let user: &MinecraftUser = &user.unwrap();

  // Attempt whitelist removal, if result is name not exist get uuid history
  let res: u8 = whitelist_account(&MinecraftUser {
    id: user.id.to_string(),
    name: user.name.to_string(),
  }, false);

  // Removal failed, look up user
  if res == 2 {
    println!("[Log] Performing deep search to remove player from whitelist");
    let uuid_history: Option<Vec<MinecraftUsernameHistory>> = get_mc_uuid_history(&user.id);

    if uuid_history.is_none() {
      println!("[WARN] NO UUID HISTORY FOUND");
      return false;
    }

    // Another overwrite
    let uuid_history: Vec<MinecraftUsernameHistory> = uuid_history.unwrap();
    // Get last value in list, assumed newest username
    let new_name: &MinecraftUsernameHistory = uuid_history.last().unwrap();
    // Get UUID from new user
    let new_uuid: Option<Vec<MinecraftUser>> = get_mc_uuid(&new_name.name);

    if new_uuid.is_none() {
      println!("[WARN] UUID NOT FOUND");
      return false;
    }

    let new_uuid: &MinecraftUser = &new_uuid.unwrap()[0];

    // Issue whitelist removal command
    let res: u8 = whitelist_account(&new_uuid, false);

    if res != 0 {
      println!("[WARN] FAILED TO REMOVE PLAYER FROM WHITELIST!");
      return false;
    }
  }

  let connection = POOL.get().unwrap();
  let num_del = diesel::delete(minecrafters.filter(discord_id.eq(discord_id)))
    .execute(&connection)
    .expect("Error deleting user by discord id");

  num_del > 0
}

fn get_mc_uuid_history(uuid: &str) -> Option<Vec<MinecraftUsernameHistory>> {
  let client = reqwest::Client::new();
  // Will panic if cannot connect to Mojang
  let address: Url = Url::parse(&format!("{}/{}/names", MOJANG_GET_HISTORY, uuid)).unwrap();
  let resp = client.get(address).send();
  match resp {
    Ok(mut val) => Some(serde_json::from_str(&val.text().unwrap()).unwrap()),
    Err(why) => {
      println!("Error retrieving profile: {:?}", why);
      None
    }
  }
}

fn get_mc_uuid(username: &str) -> Option<Vec<MinecraftUser>> {
  let client = reqwest::Client::new();
  let payload = json!([&username]);
  println!("{:#?}", payload);
  // Will panic if cannot connect to Mojang
  let resp = client.post(MOJANG_GET_UUID).json(&payload).send();
  match resp {
    Ok(mut val) => Some(serde_json::from_str(&val.text().unwrap()).unwrap()),
    Err(why) => {
      println!("Error retrieving profile: {:?}", why);
      None
    }
  }
}

#[command]
fn unlink(ctx: &mut Context, msg: &Message, _args: Args) -> CommandResult {
  let discord_vals: DiscordConfig = get_config().discord;

  // Check if channel is subscriber channel (and not a direct message)
  if &discord_vals.channel_id == msg.channel_id.as_u64() {
    msg.channel_id.broadcast_typing(&ctx)?;

    let mut response = "Your Minecraft account has been unlinked successfully.";
    let success = rem_account(*msg.author.id.as_u64());

    if !success {
      response = "You were never whitelisted or there was an error trying to unwhitelist you.";
    }

    msg.reply(
      &ctx,
      response.to_string(),
    )?;
  }

  Ok(())
}

#[command]
fn mclink(ctx: &mut Context, msg: &Message, mut args: Args) -> CommandResult {
  let discord_vals: DiscordConfig = get_config().discord;

  // Check if channel is minecraft whitelisting channel (and not a direct message)
  if &discord_vals.channel_id == msg.channel_id.as_u64() {
    // User did not reply with their Minecraft name
    if args.is_empty() {
      msg.reply(
        &ctx,
        "Please send me your Minecraft: Java Edition username.\nExample: `!mclink TheDunkel`".to_string(),
      )?;
      return Ok(());
    }
    // User sent something
    else {
      // TODO: Check if user is whitelisted already before querying to Mojang

      // Retrieve the user's current MC UUID
      let json: Option<Vec<MinecraftUser>> = get_mc_uuid(&args.single::<String>().unwrap());

      // If resulting array is empty, then username is not found
      if json.is_none() {
        msg.reply(
          &ctx,
          "Username not found. Windows 10, Mobile, and Console Editions cannot join.",
        )?;
        return Ok(());
      }

      // Overwrite json removing the Some()
      let json: Vec<MinecraftUser> = json.unwrap();

      let mut response = "There was a system issue linking your profile. Please try again later.";

      // Refer to add_account function, act accordingly
      let ret_val = add_accounts(*msg.author.id.as_u64(), &json[0]);

      match ret_val {
        Ok(1) => {
          // Issue requests to servers to whitelist
          let ret: u8 = whitelist_account(&json[0], true);
          if ret != 0 {
            response = "Unable to contact one or more game servers. Please try again later.";
            rem_account(*msg.author.id.as_u64());
          } else {
            // Assign member role
            let sender_data: Option<Member> = msg.member(&ctx.cache);
            if sender_data.is_some() {
              msg.author.direct_message(&ctx, |m| {
                // IGNORE THIS I DON'T WANT TO USE THIS RESULT
                m.content(format!(
                  "Your Minecraft account `{}` has been successfully linked.
  Please check #minecraft channel pins for server details and FAQ.
  **If you leave Mooncord for any reason, you will be removed from the whitelist**",
                  json[0].name
                ))
              })?;
            }

            return Ok(());
          }
        }
        Err(DieselError::DatabaseError(e, info)) => {
          let box_ptr = Box::into_raw(info);
          let box_val = unsafe {
            Box::from_raw(box_ptr)
          };
          let msg = box_val.message().to_string();

          match (e) {
            DatabaseErrorKind::UniqueViolation => {
              // whack
              if msg.contains("discord_id") {
                response = "You have already linked your account.\nYou may only have one linked account at a time.\nTo unlink, please type `!unlink`";
              } else if msg.contains("minecraft_uuid") {
                response = "Somebody has linked this Minecraft account already.\nPlease contact Dunkel#0001 for assistance.";
              }
            }
            _ => { }
          };
        }
        _ => { }
      };

      msg.reply(
        &ctx,
        response.to_string(),
      )?;
      return Ok(());
    }
  }

  Ok(())
}
