-- Stores an operator's first passkey, uses up their invite link, and starts
-- their session, unproven (ADR-0069).
SELECT
    identity.redeem_invite(
        $1, $2, $3, $4::text::jsonb, $5, $6, $7
    ) AS redeemed;
