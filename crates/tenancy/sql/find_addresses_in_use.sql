SELECT address
FROM tenancy.addresses
WHERE address = any($1)
ORDER BY address;
