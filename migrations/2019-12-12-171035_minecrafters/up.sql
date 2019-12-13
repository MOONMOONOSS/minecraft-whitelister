CREATE TABLE minecrafters (
    id BIGINT UNSIGNED NOT NULL PRIMARY KEY AUTO_INCREMENT,
    discord_id BIGINT UNSIGNED NOT NULL UNIQUE,
    minecraft_uuid VARCHAR(36) NOT NULL UNIQUE,
    minecraft_name VARCHAR(16) NOT NULL UNIQUE
);