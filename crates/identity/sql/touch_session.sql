-- Records a request on a session.
SELECT identity.touch_session($1, $2);
