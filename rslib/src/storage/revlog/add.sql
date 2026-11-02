INSERT
  OR IGNORE INTO revlog (
    id,
    cid,
    ease,
    ivl,
    lastIvl,
    factor,
    time,
    type
  )
VALUES (
    (
      CASE
        WHEN ?1
        AND ?2 IN (
          SELECT id
          FROM revlog
        ) THEN (
          SELECT max(id) + 1
          FROM revlog
        )
        ELSE ?2
      END
    ),
    ?,
    ?,
    ?,
    ?,
    ?,
    ?,
    ?
  )