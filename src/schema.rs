table! {
    minecrafters (id) {
        id -> Unsigned<Bigint>,
        discord_id -> Unsigned<Bigint>,
        minecraft_uuid -> Varchar,
        minecraft_name -> Varchar,
    }
}
