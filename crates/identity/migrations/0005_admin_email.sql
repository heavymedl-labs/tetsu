-- An admin's email address, for their invite link and notices of their
-- logins and passkeys (ADR-0074). Optional, since admins made before it
-- have none; such an admin gets no email, and the command still prints
-- their link. A plain address, as tetsu_kernel::Address writes it.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

ALTER TABLE admins ADD COLUMN email text CHECK (
    octet_length(email) BETWEEN 3 AND 254
    AND email !~ '[[:cntrl:][:space:]"<>]'
);

-- Identity's role adds and reads it with the name, as 0001 grants by column.
GRANT SELECT (email), INSERT (email) ON admins TO tetsu_identity;
