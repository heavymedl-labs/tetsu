-- Checks the rows 0019 left unchecked against the widened list of limits, in
-- a transaction of its own, so reads never wait on the scan.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

ALTER TABLE limits VALIDATE CONSTRAINT limits_kind_check;
