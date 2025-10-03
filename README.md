# Project Structure

```
pingora_learn/
├── Cargo.toml                    # Project configuration and dependencies
├── LICENSE                       # MIT License
├── README.md                     # Project documentation
├── config/
│   └── proxy.yaml               # Proxy service configuration
├── src/
│   ├── main.rs                  # Application entry point
│   ├── config/                  # Configuration management
│   │   ├── mod.rs
│   │   └── settings.rs          # Configuration structures and loading
│   ├── proxy/                   # Proxy core module
│   │   ├── mod.rs
│   │   ├── service.rs           # ProxyHttp trait implementation
│   │   └── context.rs           # Request context management
│   ├── middleware/              # Middleware modules
│   │   ├── mod.rs
│   │   ├── auth.rs              # Authentication middleware
│   │   └── rate_limit.rs        # Rate limiting middleware
│   └── load_balancing/          # Load balancing module
│       ├── mod.rs
│       └── manager.rs           # Load balancer manager
└── target/                      # Build output directory (generated)
```

# Pingora Proxy Service

A high-performance HTTP proxy service built with Cloudflare's Pingora framework, featuring load balancing, authentication, rate limiting, and comprehensive monitoring.

## Features

- **Load Balancing**: Round-robin, random, and least-connections strategies
- **Authentication**: Bearer token, Basic auth, and API key support
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

# Authenticated request
curl -H "Authorization: Bearer dev-token-123" http://localhost:8080

# Trigger rate limit (15 requests)
for i in {1..15}; do 
  curl -H "Authorization: Bearer dev-token-123" http://localhost:8080
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
    auth_type: "bearer"  # bearer, basic, api_key
    valid_tokens:
      - "dev-token-123"
      - "test-token-456"
  
  rate_limit:
    enabled: true
    requests_per_minute: 100
    burst_size: 10
```

## Authentication

### Bearer Token (default)
```bash
curl -H "Authorization: Bearer dev-token-123" http://localhost:8080
```

### API Key
Set `auth_type: "api_key"` in config:
```bash
curl -H "X-API-Key: dev-token-123" http://localhost:8080
```

### Basic Auth
Set `auth_type: "basic"` in config:
```bash
curl -H "Authorization: Basic dev-token-123" http://localhost:8080
```

**Note**: `/health` endpoint bypasses authentication

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
  curl -s -H "Authorization: Bearer dev-token-123" http://localhost:8080 | head -1
done
```

Watch the logs to see requests distributed across `127.0.0.1:3000`, `127.0.0.1:3001`, and `127.0.0.1:3002`.

## Response Headers

The proxy adds custom headers to all responses:

```bash
curl -I -H "Authorization: Bearer dev-token-123" http://localhost:8080
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
| 401 Unauthorized | Missing or invalid authentication | Add valid `Authorization` header |
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