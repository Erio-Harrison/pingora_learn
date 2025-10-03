# Project Structure

```
pingora_learn/
├── Cargo.toml
├── .env.example
├── .gitignore
├── README.md
├── docker-compose.yml
│
├── sql/
│   ├── 001_init_users.sql
│   └── 002_init_refresh_tokens.sql
│
├── config/
│   └── proxy.yaml
│
└── src/
    ├── main.rs
    │
    ├── config/
    │   ├── mod.rs
    │   └── settings.rs
    │
    ├── proxy/
    │   ├── mod.rs
    │   ├── service.rs  
    │   └── context.rs
    │
    ├── auth/
    │   ├── mod.rs
    │   ├── register.rs
    │   ├── login.rs
    │   ├── refresh.rs
    │   ├── logout.rs
    │   ├── jwt.rs
    │   └── password.rs
    │
    ├── db/
    │   ├── mod.rs
    │   ├── pool.rs
    │   ├── user.rs
    │   └── token.rs
    │
    ├── cache/
    │   ├── mod.rs
    │   └── client.rs
    │
    ├── middleware/
    │   ├── mod.rs
    │   ├── jwt.rs
    │   └── rate_limit.rs
    │
    └── load_balancing/
        ├── mod.rs
        └── manager.rs
```

# Pingora Proxy Service

A high-performance HTTP proxy service built with Cloudflare's Pingora framework, featuring load balancing, authentication, rate limiting, and comprehensive monitoring.

## Features

- **Load Balancing**: Round-robin, random, and least-connections strategies
- **Authentication**: JWT-based with register/login/refresh/logout support
- **Rate Limiting**: Token bucket algorithm with per-client limits
- **Health Monitoring**: Built-in health check endpoints
- **Request Tracing**: UUID-based request tracking with detailed logging

## Quick Start

### 1. Start Backend Services

```bash
# Terminal 1
python -m http.server 3000

# Terminal 2
python -m http.server 3001

# Terminal 3
python -m http.server 3002
```

### 2. Run Proxy Service

```bash
RUST_LOG=info cargo run
```

### 3. Test Requests

```bash
# Health check (no auth required)
curl http://localhost:8080/health

# Register a new user
curl -s -X POST http://localhost:8080/auth/register \
  -H "Content-Type: application/json" \
  -d '{"email":"user@example.com","password":"SecurePass123!"}'

# Login (use credentials from register response)
curl -s -X POST http://localhost:8080/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"user@example.com","password":"SecurePass123!"}'

# Authenticated request (use access_token from login response)
curl -s http://localhost:8080/ \
  -H "Authorization: Bearer YOUR_ACCESS_TOKEN_HERE"

# Trigger rate limit (15 requests)
for i in {1..15}; do 
  curl -s -H "Authorization: Bearer YOUR_ACCESS_TOKEN_HERE" http://localhost:8080
done
```

## Configuration

Edit `config/proxy.yaml`:

```yaml
server:
  listen_port: 8080
  max_connections: 1000

load_balancing:
  strategy: "round_robin"  # round_robin, random, least_conn
  upstreams:
    - name: "backend1"
      address: "127.0.0.1"
      port: 3000
      weight: 1

middleware:
  auth:
    enabled: true
    auth_type: "jwt"  # jwt (default for dynamic tokens)
  
  rate_limit:
    enabled: true
    requests_per_minute: 100
    burst_size: 10
```

## Authentication

### JWT Token Flow

1. **Register**: Create a new user account.
   ```bash
   curl -X POST http://localhost:8080/auth/register \
     -H "Content-Type: application/json" \
     -d '{"email":"user@example.com","password":"SecurePass123!"}'
   ```
   Response: `{"user_id":"uuid","email":"user@example.com","access_token":"jwt","refresh_token":"jwt","token_type":"Bearer","expires_in":900}`

2. **Login**: Authenticate and get tokens.
   ```bash
   curl -X POST http://localhost:8080/auth/login \
     -H "Content-Type: application/json" \
     -d '{"email":"user@example.com","password":"SecurePass123!"}'
   ```
   Response: Same as register.

3. **Use Access Token**: For protected requests.
   ```bash
   curl -H "Authorization: Bearer ACCESS_TOKEN" http://localhost:8080
   ```

4. **Refresh Token**: Renew access token.
   ```bash
   curl -X POST http://localhost:8080/auth/refresh \
     -H "Content-Type: application/json" \
     -d '{"refresh_token":"REFRESH_TOKEN"}'
   ```

5. **Logout**: Invalidate tokens.
   ```bash
   curl -X POST http://localhost:8080/auth/logout \
     -H "Content-Type: application/json" \
     -d '{"refresh_token":"REFRESH_TOKEN"}'
   ```

**Note**: `/health` endpoint bypasses authentication. Access tokens expire in 15 minutes; refresh tokens in 7 days.

## Load Balancing

The proxy distributes requests across multiple backend servers using configurable strategies:

### Strategies

- **round_robin**: Distributes requests sequentially across all upstreams
- **random**: Randomly selects an upstream for each request
- **least_conn**: Routes to the upstream with fewest active connections

### Example Log Output

```
[INFO] Request Start: 70605595-3c7d-4b4e-827c-8fb757b933d1 /
[INFO] Select Upstream: 70605595-3c7d-4b4e-827c-8fb757b933d1 -> 127.0.0.1:3000
[INFO] Request Completed: 70605595-3c7d-4b4e-827c-8fb757b933d1 / -> 127.0.0.1:3000 (200) (1ms)

[INFO] Request Start: a8b3f2e1-4d5c-4a1b-9e7f-8c3d2a1b0f9e /
[INFO] Select Upstream: a8b3f2e1-4d5c-4a1b-9e7f-8c3d2a1b0f9e -> 127.0.0.1:3001
[INFO] Request Completed: a8b3f2e1-4d5c-4a1b-9e7f-8c3d2a1b0f9e / -> 127.0.0.1:3001 (200) (2ms)
```

### Testing Load Balancing

```bash
# Send multiple requests to see distribution
for i in {1..10}; do
  curl -s -H "Authorization: Bearer YOUR_ACCESS_TOKEN_HERE" http://localhost:8080 | head -1
done
```

Watch the logs to see requests distributed across `127.0.0.1:3000`, `127.0.0.1:3001`, and `127.0.0.1:3002`.

## Response Headers

The proxy adds custom headers to all responses:

```bash
curl -I -H "Authorization: Bearer YOUR_ACCESS_TOKEN_HERE" http://localhost:8080
```

```
HTTP/1.0 200 OK
X-Proxy-By: Pingora-Custom-Proxy
X-Request-ID: 70605595-3c7d-4b4e-827c-8fb757b933d1
X-Response-Time: 1ms
```

## Error Responses

| Status | Reason | Solution |
|--------|--------|----------|
| 401 Unauthorized | Missing or invalid authentication | Register/login and use valid `Authorization` header |
| 429 Too Many Requests | Rate limit exceeded | Wait and retry |
| 502 Bad Gateway | Backend unavailable | Check backend services are running |

## Architecture

```
Client Request
    ↓
Authentication Check (if enabled)
    ↓
Rate Limiting Check (if enabled)
    ↓
Load Balancer Selection
    ↓
Forward to Upstream Backend
    ↓
Add Proxy Headers
    ↓
Return Response to Client
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Author

[Harrison](https://github.com/Erio-Harrison)

---

Built with ❤️ using [Pingora](https://github.com/cloudflare/pingora)