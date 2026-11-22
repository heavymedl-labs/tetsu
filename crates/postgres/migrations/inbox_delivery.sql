-- Lets the delivery role remove a consuming module's inbox IDs past their use
-- (ADR-0080), included as one of the module's own migrations after its
-- inboxes. It may read and delete them, across tenants through one policy, as
-- it does the outboxes, and never add to or change an inbox.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

DO $$
BEGIN
    EXECUTE pg_catalog.format(
        'GRANT USAGE ON SCHEMA %I TO tetsu_delivery', current_schema()
    );
END $$;

CREATE POLICY delivery ON inbox TO tetsu_delivery USING (TRUE);

GRANT SELECT, DELETE ON inbox, inbox_global TO tetsu_delivery;
