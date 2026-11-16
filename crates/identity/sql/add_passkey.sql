-- Stores an operator's added passkey.
SELECT identity.add_passkey($1, $2, $3, $4::text::jsonb) AS added;
