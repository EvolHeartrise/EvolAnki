INSERT
  OR IGNORE INTO notes (
    id,
    guid,
    mid,
    mod,
    tags,
    flds,
    sfld,
    csum,
    flags,
    data
  )
VALUES (
    ?,
    ?,
    ?,
    ?,
    ?,
    ?,
    ?,
    ?,
    0,
    ""
  )