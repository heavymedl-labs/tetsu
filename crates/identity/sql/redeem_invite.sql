-- Stores an operator's first passkey and uses up their invite link.
SELECT identity.redeem_invite($1, $2, $3, $4::text::jsonb, $5) AS redeemed;
