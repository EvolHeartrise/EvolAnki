UPDATE deck_config
SET name = ?,
  mtime_secs = ?,
  config = ?
WHERE id = ?;