#![feature(proc_macro_hygiene, decl_macro)]
use mysql::{params, Error::MySqlError, Opts, OptsBuilder};
use rocket::{get, routes};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serenity::{
  client::Client,
  framework::standard::{
    macros::{command, group},
    Args, CommandResult, StandardFramework,
  },
  model::{channel::Message, guild::Member},
  prelude::{Context, EventHandler},
};
use std::{fs::File, thread, vec};
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
const MOJANG_GET_UUID: &str = "https://api.mojang.com/profiles/minecraft/";

struct Handler;

impl EventHandler for Handler {}

#[derive(Debug, PartialEq, Eq)]
struct Account {
  discord_id: u64,
  minecraft_uuid: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct MinecraftUser {
  id: String,
  name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MinecraftUsernameHistory {
  name: String,
  changed_to_at: Option<u64>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct MinecraftServerIdentity {
  ip: String,
  port: u16,
  pass: String,
}

#[derive(Serialize, Deserialize)]
struct PatronAllResponse {
  result: String,
  users: Option<Vec<String>>,
  reason: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct PatronResponse {
  result: String,
  is_patron: Option<bool>,
  reason: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct ConfigSchema {
  discord: DiscordConfig,
  mysql: SqlConfig,
  minecraft: MinecraftConfig,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct DiscordConfig {
  guild_id: u64,
  channel_id: u64,
  token: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct SqlConfig {
  username: String,
  password: String,
  endpoint: String,
  port: u16,
  database: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct MinecraftConfig {
  servers: Vec<MinecraftServerIdentity>,
}

fn issue_cmd(conn: &mut rcon::Connection, cmd: &str) -> Option<String> {
  match conn.cmd(cmd) {
    Ok(val) => {
      println!("{}", val);

      Some(val)
    }
    Err(why) => {
      println!("RCON Failure: {:?}", why);

      None
    }
  }
}

fn get_config() -> ConfigSchema {
  let f = File::open("./config.yaml").unwrap();

  serde_yaml::from_reader(&f).unwrap()
}

fn get_all_patrons() -> Result<Vec<Account>, mysql::Error> {
  let pool = mysql::Pool::new(build_sql_opts()).unwrap();

  pool
    .prep_exec(r"SELECT discord_id, minecraft_uuid FROM minecrafters", ())
    .map(|result| {
      result
        .map(|row| {
          let (discord_id, minecraft_uuid) = mysql::from_row(row.unwrap());
          Account {
            discord_id,
            minecraft_uuid: Some(minecraft_uuid),
          }
        })
        .collect()
    })
}

#[get("/perk_eligibility/all")]
fn all_patrons() -> String {
  let mut users: Vec<String> = Vec::new();

  // Get all patron users
  let sel_user: std::result::Result<Vec<Account>, mysql::Error> = get_all_patrons();

  match sel_user {
    Ok(arr) => {
      for user in &arr {
        match &user.minecraft_uuid {
          Some(id) => {
            users.push(id.to_string());
          }
          None => {}
        };
      }
      let res: PatronAllResponse = PatronAllResponse {
        result: "success".to_string(),
        users: Some(users),
        reason: None,
      };
      serde_json::to_string(&res).unwrap()
    }
    Err(why) => {
      let res: PatronAllResponse = PatronAllResponse {
        result: "failure".to_string(),
        users: None,
        reason: Some(format!("{:#?}", why)),
      };
      serde_json::to_string(&res).unwrap()
    }
  }
}

#[get("/perk_eligibility/<minecraft_uuid>")]
fn perk_eligibility(minecraft_uuid: String) -> String {
  // let discord_vals: DiscordConfig = get_config().discord;
  let pool = mysql::Pool::new(build_sql_opts()).unwrap();
  // let http: DiscordHttp = DiscordHttp::new_with_token(&format!("Bot {}", discord_vals.token));

  // Get the user
  // Possible SQL Injection here
  let sel_user: Result<Vec<Account>, mysql::Error> = pool
    .prep_exec(
      r"
        SELECT discord_id, minecraft_uuid
        FROM minecrafters
        WHERE minecraft_uuid = :minecraft_uuid
      ",
      params! {minecraft_uuid},
    )
    .map(|result| {
      result
        .map(|row| {
          let (discord_id, minecraft_uuid) = mysql::from_row(row.unwrap());
          Account {
            discord_id,
            minecraft_uuid,
          }
        })
        .collect()
    });

  // Handle a Sql failure gracefully-ish
  match sel_user {
    Ok(arr) => {
      // If we have a result, check for Discord role
      if arr.is_empty() {
        // Unlink account if not a current Patron
        // Notify the user too if possible
        // if !is_subscriber {
        //   thread::spawn(move || {
        //     rem_account(arr[0].discord_id);

        //     let _t = discord_user.direct_message(&http, |m| {
        //       // IGNORE THIS I DON'T WANT TO USE THIS RESULT
        //       m.content("Your subscription to MOONMOON_OW has expired and your Minecraft account has been automatically unlinked.")
        //     });
        //   });
        // }

        let res = PatronResponse {
          result: "success".to_string(),
          is_patron: Some(true),
          reason: None,
        };

        serde_json::to_string(&res).unwrap();
      }
      // If we have no result, user may have changed their Minecraft Name
      // TODO: Implement name change logic on API endpoints

      let res = PatronResponse {
        result: "success".to_string(),
        is_patron: Some(false),
        reason: None,
      };
      serde_json::to_string(&res).unwrap()
    }
    Err(why) => {
      let res = PatronResponse {
        result: "failure".to_string(),
        is_patron: None,
        reason: Some(why.to_string()),
      };
      serde_json::to_string(&res).unwrap()
    }
  }
}

fn main() {
  // Start API
  thread::spawn(move || {
    rocket::ignite()
      .mount("/api/v1/twitch/", routes![perk_eligibility, all_patrons])
      .launch();
  });
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

fn build_sql_opts() -> Opts {
  let sql_vals: SqlConfig = get_config().mysql;
  let mut builder = OptsBuilder::new();
  builder
    .ip_or_hostname(Some(sql_vals.endpoint))
    .tcp_port(sql_vals.port)
    .user(Some(sql_vals.username))
    .pass(Some(sql_vals.password))
    .db_name(Some(sql_vals.database));
  builder.into()
}

fn add_accounts(discord_id: u64, mc_user: &MinecraftUser) -> u16 {
  let pool: mysql::Pool = mysql::Pool::new(build_sql_opts()).unwrap();
  // Prepare the SQL statement
  let mut stmt: mysql::Stmt = pool
    .prepare(
      r"
        INSERT INTO minecrafters
          (discord_id, minecraft_uuid, minecraft_name)
        VALUES
          (:discord_id, :minecraft_uuid, :minecraft_name)
      ",
    )
    .unwrap();
  // Execute the statement with vals
  let ret = stmt.execute(params! {
    "discord_id" => &discord_id,
    "minecraft_uuid" => &mc_user.id,
    "minecraft_name" => &mc_user.name
  });

  // This code is a nightmare, undocumented as well
  match ret {
    Ok(_val) => 0,
    Err(MySqlError(e)) => {
      if e.message.contains("Duplicate entry") {
        return e.code + 1;
      }
      e.code
    }
    Err(e) => {
      println!("SQL FAILURE: {}", e);
      1
    }
  }
}

fn whitelist_account(mc_user: &MinecraftUser) -> u8 {
  let mc_servers: Vec<MinecraftServerIdentity> = get_config().minecraft.servers;

  for server in &mc_servers {
    let address: String = format!("{}:{}", &server.ip, &server.port);
    let cmd: String = format!("whitelist add {}", mc_user.name);

    match rcon::Connection::connect(address, &server.pass) {
      Ok(mut val) => issue_cmd(&mut val, &cmd),
      Err(why) => {
        println!("Error issuing server command: {:?}", why);
        return 1;
      }
    };
  }

  0
}

fn dewhitelist_account(mc_user: &MinecraftUser) -> u8 {
  let mc_servers: Vec<MinecraftServerIdentity> = get_config().minecraft.servers;

  for server in &mc_servers {
    let address: String = format!("{}:{}", &server.ip, &server.port);
    let cmd: String = format!("whitelist remove {}", mc_user.name);

    match rcon::Connection::connect(address, &server.pass) {
      Ok(mut val) => {
        let res: String = issue_cmd(&mut val, &cmd).unwrap();
        if res == "That player does not exist" {
          return 2;
        }
      }
      Err(why) => {
        println!("Error issuing server command: {:?}", why);
        return 1;
      }
    };
  }

  0
}

fn sel_mc_account(pool: &mysql::Pool, discord_id: u64) -> Option<MinecraftUser> {
  // Prepare the SQL statement
  let mut stmt: mysql::Stmt = pool
    .prepare(
      r"
        SELECT minecraft_uuid, minecraft_name
        FROM minecrafters
        WHERE (discord_id = :discord_id)
      ",
    )
    .unwrap();
  // Execute the statement with vals
  let res: Result<Vec<MinecraftUser>, mysql::Error> = stmt
    .execute(params! {
      "discord_id" => &discord_id
    })
    .map(|result| {
      result
        .map(|row| {
          let (uuid, name) = mysql::from_row(row.unwrap());
          MinecraftUser {
            id: uuid,
            name,
          }
        })
        .collect()
    });

  match res {
    Ok(arr) => {
      if !arr.is_empty() {
        arr[0].id.to_string();arr[0].name.to_string();
      }
      println!("[WARN] NO PLAYER FOUND BY DISCORD ID");

      None
    }
    Err(why) => {
      println!("Error while selecting accounts: {:?}", why);
      None
    }
  }
}

fn rem_account(discord_id: u64) {
  let pool: mysql::Pool = mysql::Pool::new(build_sql_opts()).unwrap();

  // Retrieve MC account for whitelist removal
  let user: Option<MinecraftUser> = sel_mc_account(&pool, discord_id);

  if user.is_none() {
    // User was never whitelisted or manually removed
    return;
  }

  // Overwrite with val
  let user: &MinecraftUser = &user.unwrap();

  // Attempt whitelist removal, if result is name not exist get uuid history
  let res: u8 = dewhitelist_account(&MinecraftUser {
    id: user.id.to_string(),
    name: user.name.to_string(),
  });

  // Removal failed, look up user
  if res == 2 {
    println!("[Log] Performing deep search to remove player from whitelist");
    let uuid_history: Option<Vec<MinecraftUsernameHistory>> = get_mc_uuid_history(&user.id);

    if uuid_history.is_none() {
      println!("[WARN] NO UUID HISTORY FOUND");
      return;
    }

    // Another overwrite
    let uuid_history: Vec<MinecraftUsernameHistory> = uuid_history.unwrap();
    // Get last value in list, assumed newest username
    let new_name: &MinecraftUsernameHistory = uuid_history.last().unwrap();
    // Get UUID from new user
    let new_uuid: Option<Vec<MinecraftUser>> = get_mc_uuid(&new_name.name);

    if new_uuid.is_none() {
      println!("[WARN] UUID NOT FOUND");
      return;
    }

    let new_uuid: &MinecraftUser = &new_uuid.unwrap()[0];

    // Issue whitelist removal command
    let res: u8 = dewhitelist_account(&new_uuid);

    if res != 0 {
      println!("[WARN] FAILED TO REMOVE PLAYER FROM WHITELIST!");
      return;
    }
  }

  // Prepare the SQL statement
  let mut stmt: mysql::Stmt = pool
    .prepare(
      r"DELETE FROM minecrafters WHERE
    (discord_id = :discord_id)",
    )
    .unwrap();
  // Execute the statement with vals
  stmt
    .execute(params! {
      "discord_id" => &discord_id
    })
    .unwrap();
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

    rem_account(*msg.author.id.as_u64());

    msg.reply(
      &ctx,
      "Your Minecraft account has been unlinked successfully.",
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

      // Refer to add_account function, act accordingly
      let ret_val: u16 = add_accounts(*msg.author.id.as_u64(), &json[0]);
      match ret_val {
        0 => {
          // Issue requests to servers to whitelist
          let ret: u8 = whitelist_account(&json[0]);
          if ret != 0 {
            msg.reply(
              &ctx,
              "Unable to contact one or more game servers. Please try again later.",
            )?;
            rem_account(*msg.author.id.as_u64());
            return Ok(());
          }
          // Assign member role
          let sender_data: Option<Member> = msg.member(&ctx.cache);
          if sender_data.is_some() {
            let mut sender_data: Member = sender_data.unwrap();
            sender_data.add_role(&ctx.http, 597_630_558_733_860_866)?;
            msg.author.direct_message(&ctx, |m| {
              // IGNORE THIS I DON'T WANT TO USE THIS RESULT
              m.content(format!(
                "Your Minecraft account `{}` has been successfully linked.
Please check #minecraft channel pins for server details, modpack, and FAQ.
Please see #minecraft_resources on how to join the Minecraft Alpha server!",
                json[0].name
              ))
            })?;
          }
          return Ok(());
        }
        1062 => {
          msg.reply(
            &ctx,
            "You have already linked your account.\nYou may only have one linked account at a time.\nTo unlink, please type `!unlink`".to_string(),
          )?;
          return Ok(());
        }
        1063 => {
          msg.reply(
            &ctx,
            "Somebody has linked this Minecraft account already.\nPlease contact Dunkel#0001 for assistance.".to_string(),
          )?;
          return Ok(());
        }
        _ => {
          msg.reply(
            &ctx,
            "There was a system issue linking your profile. Please try again later.".to_string(),
          )?;
          return Ok(());
        }
      };
    }
  }

  Ok(())
}
