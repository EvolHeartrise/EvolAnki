SELECT ord,
  name,
  mtime_secs,
  config
FROM templates
WHERE ntid = ?
ORDER BY ord