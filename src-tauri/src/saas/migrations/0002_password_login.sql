ALTER TABLE saas_users ADD COLUMN email TEXT;
ALTER TABLE saas_users ADD COLUMN password_hash TEXT;

CREATE UNIQUE INDEX saas_users_email
  ON saas_users(lower(email))
  WHERE email IS NOT NULL;
