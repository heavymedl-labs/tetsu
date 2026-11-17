-- Takes a login ceremony, once, if it is still open.
SELECT c.state FROM identity.take_ceremony($1, 'login') AS c;
