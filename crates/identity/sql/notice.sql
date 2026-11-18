-- Who to tell about an operator's login or new passkey, and when it was, in
-- words, so the notice needs no clock of its own.
SELECT
    email,
    to_char(
        $2::timestamptz AT TIME ZONE 'UTC',
        'FMMonth FMDD, YYYY, HH24:MI "UTC"'
    ) AS occurred
FROM identity.operators
WHERE id = $1;
