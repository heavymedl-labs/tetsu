SELECT
    name,
    time_zone
FROM tenancy.locations
WHERE tenant_id = $1
ORDER BY sort_order;
