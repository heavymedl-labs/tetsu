-- Stores an operator's first passkey and uses up their enrollment link.
SELECT identity.redeem_enrollment($1, $2, $3, $4::text::jsonb, $5) AS redeemed;
