-- Stores a staff member's first passkey, uses up their invite and gives them
-- its role in this business, starts their session, unproven (ADR-0069), and
-- names its device.
SELECT
    r.redeemed,
    identity.staff_describe_session($6, $8) AS described
FROM
    identity.staff_join($1, $2, $3, $4::text::jsonb, $5, $6, $7)
        AS r (redeemed);
