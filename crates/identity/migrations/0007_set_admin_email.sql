-- An admin made before addresses, or whose address changes, is given one
-- by the admin command (ADR-0074). Identity's role may change only that
-- column, as 0005 lets it add and read it.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

GRANT UPDATE (email) ON admins TO tetsu_identity;
