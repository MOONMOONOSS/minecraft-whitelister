use super::schema::minecrafters;
use serde::{Deserialize, Serialize};
use std::{error:Error, fmt}

#[derive(Debug, PartialEq, Eq)]
pub struct Account {
  pub discord_id: u64,
  pub minecraft_uuid: Option<String>,
}

#[derive(Queryable, Identifiable)]
pub struct FullMCUser {
  pub id: u64,
  pub discord_id: u64,
  pub minecraft_uuid: String,
  pub minecraft_name: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinecraftUser {
  pub id: String,
  pub name: String,
}

#[derive(Insertable)]
#[table_name = "minecrafters"]
pub struct NewMinecraftUser {
  pub discord_id: u64,
  pub minecraft_uuid: String,
  pub minecraft_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MinecraftUsernameHistory {
  pub name: String,
  pub changed_to_at: Option<u64>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct MinecraftServerIdentity {
  pub ip: String,
  pub port: u16,
  pub pass: String,
}

#[derive(Serialize, Deserialize)]
pub struct PatronAllResponse {
  pub result: String,
  pub users: Option<Vec<String>>,
  pub reason: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct PatronResponse {
  pub result: String,
  pub is_patron: Option<bool>,
  pub reason: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ConfigSchema {
  pub discord: DiscordConfig,
  pub minecraft: MinecraftConfig,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct DiscordConfig {
  pub guild_id: u64,
  pub channel_id: u64,
  pub token: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SqlConfig {
  pub username: String,
  pub password: String,
  pub endpoint: String,
  pub port: u16,
  pub database: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct MinecraftConfig {
  pub servers: Vec<MinecraftServerIdentity>,
}


#[derive(Debug)]
struct WhitelistError;

impl fmt::Display for WhitelistError {
  fn fmt(&self, f &mut fmt::Formatter) -> fmt::Result {
    write!(f, "")
  }
}