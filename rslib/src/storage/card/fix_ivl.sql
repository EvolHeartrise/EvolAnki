UPDATE cards
SET ivl = min(max(round(ivl), 0), 2147483647),
  mod = ?1
WHERE ivl != min(max(round(ivl), 0), 2147483647)