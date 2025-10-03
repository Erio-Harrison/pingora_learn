CREATE TABLE IF NOT EXISTS refresh_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash VARCHAR(255) UNIQUE NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL 
);

CREATE INDEX IF NOT EXISTS idx_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_tokens_token_hash ON refresh_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_tokens_expires_at ON refresh_tokens(expires_at);