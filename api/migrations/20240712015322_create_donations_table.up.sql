-- Ensure the uuid-ossp extension is enabled
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE users (
  id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
  email VARCHAR(100) UNIQUE NOT NULL
);

INSERT INTO users (id, email)
VALUES (uuid_generate_v4(), 'admin')
ON CONFLICT (email) DO NOTHING;

CREATE TABLE wallets (
  id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
  data JSONB NOT NULL,
  is_active BOOLEAN NOT NULL DEFAULT false,
  user_id uuid REFERENCES users(id) NOT NULL
);

CREATE TABLE donations (
  id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
  amount DECIMAL(10, 2) NOT NULL,
  title VARCHAR(100) NOT NULL,
  description TEXT,
  webhook VARCHAR(255),
  wallet_id uuid REFERENCES wallets(id) DEFAULT NULL,
  user_id uuid REFERENCES users(id) NOT NULL
);

CREATE TABLE transactions (
  id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
  wallet_id uuid REFERENCES wallets(id) NOT NULL,
  amount DECIMAL(16, 2) NOT NULL,
  type VARCHAR(20) CHECK (type IN ('deposit', 'withdrawal')),  -- Enforce transaction type
  date TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
