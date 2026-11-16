-- Takes a sign-in ceremony, once, if it is still open.
SELECT c.state FROM identity.take_ceremony($1, 'sign_in') AS c;
