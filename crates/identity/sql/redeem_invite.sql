-- Stores an operator's first passkey, uses up their invite link, starts
-- their session, unproven (ADR-0069), and names its device.
SELECT
    r.redeemed,
    identity.describe_session($6, $8) AS described
FROM
    identity.redeem_invite($1, $2, $3, $4::text::jsonb, $5, $6, $7)
        AS r (redeemed);
