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
- **Configurable**: YAML-based configuration management

## Usage

### Setup Backend Services

Start test servers on the configured ports:

```bash
# Terminal 1 - Backend on port 3000
python -m http.server 3000

# Terminal 2 - Backend on port 3001  
python -m http.server 3001

# Terminal 3 - Backend on port 3002
python -m http.server 3002
```

Alternatively, use Node.js:
```bash
# Install serve globally
npm install -g serve

# Start backends
serve -p 3000
serve -p 3001  
serve -p 3002
```

Or use any web server of your choice on these ports.

### Basic Requests
```bash
# Proxy request
curl http://localhost:8080

# Health check
curl http://localhost:8080/health

# View response headers
curl -I http://localhost:8080
```

### Authentication
```bash
# Bearer token
curl -H "Authorization: Bearer your-token" http://localhost:8080

# API key
curl -H "X-API-Key: your-key" http://localhost:8080
```

### Rate Limiting Test
```bash
# Multiple requests to trigger rate limit
for i in {1..15}; do curl http://localhost:8080; done
```

## Architecture

### Core Components

- **ProxyService**: Main proxy logic implementing Pingora's ProxyHttp trait
- **LoadBalancingManager**: Handles upstream server selection
- **AuthMiddleware**: Validates authentication credentials  
- **RateLimitMiddleware**: Enforces rate limits using token bucket algorithm
- **ProxyContext**: Tracks per-request state and metadata

### Request Flow

1. **Request Reception** → Proxy receives client request
2. **Authentication** → Validates credentials (if enabled)
3. **Rate Limiting** → Checks request frequency limits
4. **Load Balancing** → Selects upstream server
5. **Request Forwarding** → Sends request to backend
6. **Response Processing** → Adds proxy headers and logging
7. **Response Return** → Returns response to client

## Configuration Reference

### Server Configuration
```yaml
server:
  listen_port: 8080           # Proxy listening port
  max_connections: 1000       # Maximum concurrent connections
```

### Load Balancing
```yaml
load_balancing:
  strategy: "round_robin"     # round_robin, random, least_conn
  upstreams:
    - name: "backend1"
      address: "127.0.0.1"
      port: 3000
      weight: 1               # Load balancing weight
```

### Authentication
```yaml
middleware:
  auth:
    enabled: true
    auth_type: "bearer"       # bearer, basic, api_key
    valid_tokens: ["token1", "token2"]
```

### Rate Limiting
```yaml
middleware:
  rate_limit:
    enabled: true
    requests_per_minute: 100  # Token refill rate
    burst_size: 10           # Initial token capacity
```

## Monitoring

### Request Tracing
Each request receives a unique UUID for tracking:
```
[INFO] Request start: a1b2c3d4-... /api/users
[INFO] Selected upstream: a1b2c3d4-... -> 127.0.0.1:3000
[INFO] Request complete: a1b2c3d4-... /api/users -> 127.0.0.1:3000 (200) (15ms)
```

### Health Endpoint
```bash
curl http://localhost:8080/health
# Returns: {"status": "healthy"}
```

## Performance

- Built on Pingora's high-performance networking stack
- Async/await throughout for non-blocking I/O
- Efficient memory usage with zero-copy where possible
- Configurable connection limits and timeouts

## License

MIT License - see [LICENSE](LICENSE) for details.

## Author

[Harrison](https://github.com/Erio-Harrison)

---

Built with ❤️ using [Pingora](https://github.com/cloudflare/pingora)