SELECT id,
  nid,
  ord,
  cast(mod AS integer),
  did
FROM cards
WHERE did = ?
  AND queue = 0