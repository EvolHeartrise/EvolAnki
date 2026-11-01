SELECT id,
  due,
  cast(ivl AS integer),
  cast(mod AS integer),
  did,
  reps
FROM cards
WHERE did IN (
    SELECT id
    FROM active_decks
  )
  AND (
    queue = ?
    AND due <= ?
  )