-- Adds an operator with no passkey.
INSERT INTO identity.operators (name) VALUES ($1) RETURNING id;
