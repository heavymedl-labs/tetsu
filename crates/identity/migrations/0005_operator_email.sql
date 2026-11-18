-- An operator's email address, for their invite link and notices of their
-- logins and passkeys (ADR-0074). Optional, since operators made before it
-- have none; such an operator gets no email, and the command still prints
-- their link. A plain address, as tetsu_kernel::Address writes it.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

ALTER TABLE operators ADD COLUMN email text CHECK (
    octet_length(email) BETWEEN 3 AND 254
    AND email !~ '[[:cntrl:][:space:]"<>]'
);

-- Identity's role adds and reads it with the name, as 0001 grants by column.
GRANT SELECT (email), INSERT (email) ON operators TO tetsu_identity;
