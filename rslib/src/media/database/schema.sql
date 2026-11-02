CREATE TABLE media (
  fname text NOT NULL PRIMARY KEY,
  csum text,
  mtime int NOT NULL
) without rowid;
CREATE TABLE meta (dirMod int);
INSERT INTO meta
VALUES (0);