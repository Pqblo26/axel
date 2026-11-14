-- Sub members and special skill of each formation of the dungeon team, by main member:
-- [{"main": id, "sub1": id, "sub2": id, "special": skill id}]
alter table user_dungeon_teams add column if not exists forms jsonb not null default '[]';
