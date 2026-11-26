-- Who to tell about a staff member's login or new passkey, and when it was,
-- in words, so the notice needs no clock of its own.
SELECT
    identity.staff_email($1) AS email,
    to_char(
        $2::timestamptz AT TIME ZONE 'UTC',
        'FMMonth FMDD, YYYY, HH24:MI "UTC"'
    ) AS occurred;
