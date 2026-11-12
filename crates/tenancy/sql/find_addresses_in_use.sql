SELECT DISTINCT wanted.address
FROM unnest($1::text[]) AS wanted (address)
WHERE tenancy.address_in_use(wanted.address)
ORDER BY wanted.address;
