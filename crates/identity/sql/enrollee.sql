-- The operator an open enrollment link names, if they may enroll.
SELECT
    e.operator_id,
    e.user_handle,
    e.name
FROM identity.enrollee($1) AS e;
