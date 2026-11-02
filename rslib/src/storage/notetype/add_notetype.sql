INSERT INTO notetypes (id, name, mtime_secs, config)
VALUES (
    (
      CASE
        WHEN ?1 IN (
          SELECT id
          FROM notetypes
        ) THEN (
          SELECT max(id) + 1
          FROM notetypes
        )
        ELSE ?1
      END
    ),
    ?,
    ?,
    ?
  );